//! Export PowerPoint PresentationML minimal, sans bibliothèque Office externe.
use crate::{geometry::{self, Measurement, Point, Tool}, report::{self, FolderReportOptions, FolderReportPhoto}};
use image::ImageEncoder;
use std::{fs::{self, File, OpenOptions}, io::{BufWriter, Write}, path::{Path, PathBuf}, sync::Arc, time::{SystemTime, UNIX_EPOCH}};

const SLIDE_W: i64 = 12_192_000;
const SLIDE_H: i64 = 6_858_000;
const TABLE_X: i64 = 410_000;
const TABLE_W: i64 = SLIDE_W - TABLE_X * 2;
const TABLE_COLS: [i64; 5] = [1_600_000, 1_500_000, 3_100_000, 1_900_000, 3_272_000];
const GROUP_TABLE_COLS: [i64; 6] = [2_650_000, 1_320_000, 1_720_000, 1_950_000, 1_450_000, 2_282_000];
const PALETTE: [&str; 8] = ["009FC7", "E69E00", "C7334D", "29915C", "734FBA", "D95C1F", "2E6BB8", "A64F87"];
const MAX_GROUP_MEASUREMENTS: usize = 6;

struct SlideSpec {
    photo: usize,
    photo_number: usize,
    content: SlideContent,
}

enum SlideContent {
    Photo,
    GroupOverview { groups: Vec<NumberedReportGroup>, first_group: usize, group_count: usize },
    Group { group: NumberedReportGroup, first_row: usize, row_count: usize },
    GroupAnnotations { group: NumberedReportGroup, first_annotation: usize, annotation_count: usize },
    Reference { page: Option<usize>, first_row: usize, row_count: usize },
    Audit { lines: Arc<Vec<AuditLine>>, first_line: usize, line_count: usize },
}

#[derive(Clone)]
struct AuditLine { label: String, value: String }

#[derive(Clone)]
struct NumberedReportGroup {
    number: usize,
    group: ReportGroup,
}

#[derive(Clone, Default)]
struct ReportGroup {
    description: String,
    measurement_ids: Vec<u64>,
    dimension_indices: Vec<usize>,
    annotation_indices: Vec<usize>,
}

const REFERENCE_ROWS_PER_SLIDE: usize = 7;

pub fn write_folder_pptx(
    path: &Path,
    options: &FolderReportOptions,
    photos: &[FolderReportPhoto],
) -> Result<Option<String>, String> {
    if photos.is_empty() { return Err("Sélectionnez au moins une image pour la présentation".into()); }
    for photo in photos {
        let expected = (photo.width as usize).checked_mul(photo.height as usize)
            .and_then(|pixels| pixels.checked_mul(3))
            .ok_or_else(|| format!("Dimensions trop grandes : {}", photo.name))?;
        if photo.width == 0 || photo.height == 0 || photo.image_rgb.len() != expected {
            return Err(format!("Pixels invalides : {}", photo.name));
        }
    }

    let mut slides = Vec::new();
    let mut next_group_number = 1usize;
    for (photo_index, photo) in photos.iter().enumerate() {
        let mut numbered_groups = if options.include_measurements {
            build_pptx_groups(photo).into_iter().map(|group| {
                let numbered = NumberedReportGroup { number: next_group_number, group };
                next_group_number += 1;
                numbered
            }).collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        if options.include_measurements && !numbered_groups.is_empty() {
            const GROUPS_PER_OVERVIEW: usize = 8;
            for first_group in (0..numbered_groups.len()).step_by(GROUPS_PER_OVERVIEW) {
                let group_count = numbered_groups.len().saturating_sub(first_group).min(GROUPS_PER_OVERVIEW);
                slides.push(SlideSpec {
                    photo: photo_index,
                    photo_number: photo_index + 1,
                    content: SlideContent::GroupOverview {
                        groups: numbered_groups.clone(), first_group, group_count,
                    },
                });
            }
            for group in numbered_groups.drain(..) {
                let row_count = group_comparison_rows(photo, &group.group).len().max(1);
                const GROUP_ROWS_PER_SLIDE: usize = 6;
                for first_row in (0..row_count).step_by(GROUP_ROWS_PER_SLIDE) {
                    slides.push(SlideSpec {
                        photo: photo_index,
                        photo_number: photo_index + 1,
                        content: SlideContent::Group {
                            group: group.clone(),
                            first_row,
                            row_count: row_count.saturating_sub(first_row).min(GROUP_ROWS_PER_SLIDE),
                        },
                    });
                }
                if group.group.annotation_indices.len() > 6 {
                    const GROUP_ANNOTATIONS_PER_SLIDE: usize = 4;
                    for first_annotation in (0..group.group.annotation_indices.len())
                        .step_by(GROUP_ANNOTATIONS_PER_SLIDE)
                    {
                        slides.push(SlideSpec {
                            photo: photo_index,
                            photo_number: photo_index + 1,
                            content: SlideContent::GroupAnnotations {
                                group: group.clone(),
                                first_annotation,
                                annotation_count: group.group.annotation_indices.len()
                                    .saturating_sub(first_annotation)
                                    .min(GROUP_ANNOTATIONS_PER_SLIDE),
                            },
                        });
                    }
                }
            }
        } else {
            slides.push(SlideSpec {
                photo: photo_index,
                photo_number: photo_index + 1,
                content: SlideContent::Photo,
            });
        }
        if let Some(reference) = &photo.reference {
            let row_chunks = reference.dimensions.len().div_ceil(REFERENCE_ROWS_PER_SLIDE).max(1);
            for page_index in 0..reference.pages.len() {
                let row_count = if page_index == 0 { reference.dimensions.len().min(REFERENCE_ROWS_PER_SLIDE) } else { 0 };
                slides.push(SlideSpec {
                    photo: photo_index, photo_number: photo_index + 1,
                    content: SlideContent::Reference { page: Some(page_index), first_row: 0, row_count },
                });
            }
            for row_chunk in if reference.pages.is_empty() { 0 } else { 1 }..row_chunks {
                let first_row = row_chunk * REFERENCE_ROWS_PER_SLIDE;
                let row_count = reference.dimensions.len().saturating_sub(first_row)
                    .min(REFERENCE_ROWS_PER_SLIDE);
                slides.push(SlideSpec {
                    photo: photo_index, photo_number: photo_index + 1,
                    content: SlideContent::Reference { page: None, first_row, row_count },
                });
            }
        }
    }
    if options.include_audit_page || photos.iter().any(|photo| !crate::report::inspection_export_notes(&photo.session).is_empty()) {
        let audit_lines = Arc::new(build_audit_lines(options, photos));
        const AUDIT_LINES_PER_SLIDE: usize = 28;
        for first_line in (0..audit_lines.len().max(1)).step_by(AUDIT_LINES_PER_SLIDE) {
            slides.push(SlideSpec {
                photo: 0,
                photo_number: 1,
                content: SlideContent::Audit {
                    lines: audit_lines.clone(),
                    first_line,
                    line_count: audit_lines.len().saturating_sub(first_line).min(AUDIT_LINES_PER_SLIDE),
                },
            });
        }
    }
    if slides.len() > u16::MAX as usize { return Err("La présentation dépasse le nombre maximal de diapositives pris en charge".into()); }

    let reference_image_count = photos.iter().filter_map(|photo| photo.reference.as_ref())
        .map(|reference| reference.pages.len()).sum::<usize>();
    let entry_count = 10usize.checked_add(photos.len())
        .and_then(|count| count.checked_add(reference_image_count))
        .and_then(|count| slides.len().checked_mul(2).and_then(|slide_entries| count.checked_add(slide_entries)))
        .ok_or_else(|| "La présentation dépasse les limites ZIP prises en charge".to_owned())?;
    if entry_count > u16::MAX as usize {
        return Err("La présentation dépasse le nombre maximal de fichiers pris en charge dans ZIP32".into());
    }
    let temp_path = temporary_output_path(path, "tmp");
    let temp_file = OpenOptions::new().write(true).create_new(true).open(&temp_path)
        .map_err(|error| format!("Création du fichier temporaire PowerPoint impossible : {error}"))?;
    let result = (|| {
    let mut archive = StoredZip::new(temp_file);
    archive.add("[Content_Types].xml", content_types(slides.len()).into_bytes())?;
    archive.add("_rels/.rels", root_relationships().into_bytes())?;
    archive.add("ppt/presentation.xml", presentation_xml(slides.len()).into_bytes())?;
    archive.add("ppt/_rels/presentation.xml.rels", presentation_relationships(slides.len()).into_bytes())?;
    archive.add("ppt/presProps.xml", b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:presentationPr xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"/>".to_vec())?;
    archive.add("ppt/slideMasters/slideMaster1.xml", slide_master().into_bytes())?;
    archive.add("ppt/slideMasters/_rels/slideMaster1.xml.rels", master_relationships().into_bytes())?;
    archive.add("ppt/slideLayouts/slideLayout1.xml", slide_layout().into_bytes())?;
    archive.add("ppt/slideLayouts/_rels/slideLayout1.xml.rels", layout_relationships().into_bytes())?;
    archive.add("ppt/theme/theme1.xml", theme().into_bytes())?;

    for (index, photo) in photos.iter().enumerate() {
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&photo.image_rgb, photo.width, photo.height, image::ExtendedColorType::Rgb8)
            .map_err(|error| format!("Encodage PNG impossible pour {} : {error}", photo.name))?;
        archive.add(format!("ppt/media/image{}.png", index + 1), png)?;
        if let Some(reference) = &photo.reference {
            for (page_index, page) in reference.pages.iter().enumerate() {
                let mut png = Vec::new();
                image::codecs::png::PngEncoder::new(&mut png)
                    .write_image(&page.rgb, page.width, page.height, image::ExtendedColorType::Rgb8)
                    .map_err(|error| format!("Encodage du document de référence impossible : {error}"))?;
                archive.add(format!("ppt/media/reference{}_{}.png", index + 1, page_index + 1), png)?;
            }
        }
    }
    for (index, spec) in slides.iter().enumerate() {
        let xml = slide_xml(options, photos, spec, index, slides.len());
        archive.add(format!("ppt/slides/slide{}.xml", index + 1), xml.into_bytes())?;
        archive.add(format!("ppt/slides/_rels/slide{}.xml.rels", index + 1),
            slide_relationships(spec).into_bytes())?;
    }
    archive.finish()
    })();
    let result = match result {
        Ok(()) => replace_completed_output(&temp_path, path),
        Err(error) => Err(error),
    };
    match result {
        Ok(warning) => Ok(warning),
        Err(error) => match fs::remove_file(&temp_path) {
            Ok(()) => Err(error),
            Err(cleanup_error) if temp_path.exists() => Err(format!(
                "{error} ; fichier temporaire conservé à {} ({cleanup_error})", temp_path.display())),
            Err(_) => Err(error),
        },
    }
}

fn temporary_output_path(destination: &Path, suffix: &str) -> PathBuf {
    let file_name = destination.file_name().and_then(|name| name.to_str()).unwrap_or("dper_measure_report");
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    destination.with_file_name(format!(".{file_name}.{}.{stamp}.{suffix}", std::process::id()))
}

fn replace_completed_output(temporary: &Path, destination: &Path) -> Result<Option<String>, String> {
    if !destination.exists() {
        return fs::rename(temporary, destination)
            .map(|()| None)
            .map_err(|error| format!("Finalisation du PowerPoint impossible : {error}"));
    }
    if !destination.is_file() {
        return Err("Le chemin de sortie PowerPoint existe et n'est pas un fichier".into());
    }

    let backup = temporary_output_path(destination, "backup");
    fs::rename(destination, &backup)
        .map_err(|error| format!("Impossible de remplacer le rapport existant : {error}"))?;
    match fs::rename(temporary, destination) {
        Ok(()) => match fs::remove_file(&backup) {
            Ok(()) => Ok(None),
            Err(error) => Ok(Some(format!(
                "L'ancienne version reste dans {} ({error})", backup.display()))),
        },
        Err(error) => match fs::rename(&backup, destination) {
            Ok(()) => Err(format!("Finalisation du PowerPoint impossible : {error}")),
            Err(restore_error) => Err(format!(
                "Finalisation impossible ({error}) ; l'ancien rapport est conservé ici : {} ({restore_error})",
                backup.display())),
        },
    }
}

fn build_pptx_groups(photo: &FolderReportPhoto) -> Vec<ReportGroup> {
    let mut groups = Vec::<ReportGroup>::new();
    if let Some(reference) = &photo.reference {
        for (dimension_index, dimension) in reference.dimensions.iter().enumerate() {
            if dimension.measurement_id.is_some_and(|id| photo.session.measurements.iter()
                .any(|measurement| measurement.id == id && measurement.is_construction)) { continue; }
            let description = group_characteristic(&dimension.label)
                .or_else(|| dimension.measurement_id
                    .and_then(|id| photo.session.measurements.iter().find(|measurement| measurement.id == id))
                    .map(|measurement| measurement.kind.label().to_owned()))
                .unwrap_or_else(|| "Autres cotes de référence".to_owned());
            let group_index = report_group_index(&mut groups, &description);
            groups[group_index].dimension_indices.push(dimension_index);
            if let Some(id) = dimension.measurement_id.filter(|id| {
                photo.session.measurements.iter().any(|measurement| measurement.id == *id)
            }) {
                push_unique(&mut groups[group_index].measurement_ids, id);
            }
        }
    }
    for measurement in photo.session.measurements.iter().filter(|measurement| !measurement.is_construction) {
        if groups.iter().any(|group| group.measurement_ids.contains(&measurement.id)) { continue; }
        let description = group_characteristic(&measurement.label)
            .unwrap_or_else(|| measurement.kind.label().to_owned());
        let group_index = report_group_index(&mut groups, &description);
        push_unique(&mut groups[group_index].measurement_ids, measurement.id);
    }
    if groups.is_empty() {
        for measurement in photo.session.measurements.iter().filter(|measurement| !measurement.is_construction) {
            let group_index = report_group_index(&mut groups, measurement.kind.label());
            push_unique(&mut groups[group_index].measurement_ids, measurement.id);
        }
    }
    let mut groups = groups.into_iter().flat_map(|group| split_pptx_group(group, photo)).collect::<Vec<_>>();

    for (annotation_index, annotation) in photo.session.annotations.iter().enumerate() {
        let target_measurement_id = match &annotation.target {
            crate::geometry::AnnotationTarget::Measurement { measurement_id } => Some(*measurement_id),
            _ => None,
        };
        let containing_group = target_measurement_id.and_then(|id| groups.iter()
            .position(|group| group.measurement_ids.contains(&id)));
        let target = annotation_anchor(annotation, &photo.session.measurements);
        let nearest_group = target.and_then(|target| groups.iter().enumerate()
            .filter(|(_, group)| !group.measurement_ids.is_empty())
            .min_by(|(_, left), (_, right)| {
                let left_center = report_group_center(left, photo);
                let right_center = report_group_center(right, photo);
                (left_center.0 - target.x).hypot(left_center.1 - target.y)
                    .total_cmp(&(right_center.0 - target.x).hypot(right_center.1 - target.y))
            }).map(|(index, _)| index));
        if let Some(group_index) = containing_group.or(nearest_group).or_else(|| (!groups.is_empty()).then_some(0)) {
            groups[group_index].annotation_indices.push(annotation_index);
        }
    }
    groups
}

fn report_group_index(groups: &mut Vec<ReportGroup>, description: &str) -> usize {
    if let Some(index) = groups.iter().position(|group| group.description == description) { return index; }
    groups.push(ReportGroup { description: description.to_owned(), ..ReportGroup::default() });
    groups.len() - 1
}

fn push_unique(values: &mut Vec<u64>, value: u64) {
    if !values.contains(&value) { values.push(value); }
}

fn group_characteristic(label: &str) -> Option<String> {
    let normalized = label.to_lowercase();
    if normalized.contains("pas x") {
        let base = group_label_prefix(label);
        return Some(number_after_marker(label, &["rangée", "rangee"])
            .map(|row| format!("{base} · rangée {row}")).unwrap_or_else(|| base.to_owned()));
    }
    if normalized.contains("pas y") {
        let base = group_label_prefix(label);
        return Some(number_after_marker(label, &["colonne"])
            .map(|column| format!("{base} · colonne {column}")).unwrap_or_else(|| base.to_owned()));
    }
    if normalized.contains("décalage x") || normalized.contains("decalage x")
        || normalized.contains("décalage y") || normalized.contains("decalage y") {
        return Some(group_label_prefix(label).to_owned());
    }
    if normalized.contains("hors tout x") { return Some("Dimensions hors tout X".to_owned()); }
    if normalized.contains("hors tout y") { return Some("Dimensions hors tout Y".to_owned()); }
    if normalized.contains("hors tout") { return Some("Dimensions hors tout".to_owned()); }
    if normalized.contains("enveloppe motifs x") { return Some("Enveloppe des motifs X".to_owned()); }
    if normalized.contains("enveloppe motifs y") { return Some("Enveloppe des motifs Y".to_owned()); }
    if normalized.contains("enveloppe") { return Some("Enveloppe des motifs".to_owned()); }
    if normalized.contains("motif") || normalized.contains("logement")
        || normalized.contains("profondeur")
    {
        return Some(group_label_prefix(label).to_owned());
    }
    None
}

fn group_label_prefix(label: &str) -> &str {
    [" — ", " – ", " - "].iter().find_map(|&separator| label.find(separator).map(|index| label[..index].trim()))
        .filter(|prefix| !prefix.is_empty()).unwrap_or_else(|| label.trim())
}

fn number_after_marker(text: &str, markers: &[&str]) -> Option<String> {
    let normalized = text.to_lowercase();
    for marker in markers {
        let Some(index) = normalized.find(marker) else { continue; };
        let digits = normalized[index + marker.len()..].chars()
            .skip_while(|ch| !ch.is_ascii_digit()).take_while(|ch| ch.is_ascii_digit()).collect::<String>();
        if !digits.is_empty() { return Some(digits); }
    }
    None
}

fn split_pptx_group(group: ReportGroup, photo: &FolderReportPhoto) -> Vec<ReportGroup> {
    let measurements = photo.session.measurements.iter()
        .filter(|measurement| group.measurement_ids.contains(&measurement.id)).collect::<Vec<_>>();
    if measurements.len() <= MAX_GROUP_MEASUREMENTS { return vec![group]; }
    let zones = split_measurements_spatially(measurements, MAX_GROUP_MEASUREMENTS);
    let mut split = Vec::with_capacity(zones.len() + 1);
    for (zone_index, ids) in zones.into_iter().enumerate() {
        let mut zone = ReportGroup {
            description: format!("{} · zone {}", group.description, zone_index + 1),
            measurement_ids: ids,
            ..ReportGroup::default()
        };
        for dimension_index in &group.dimension_indices {
            let measurement_id = photo.reference.as_ref()
                .and_then(|reference| reference.dimensions.get(*dimension_index))
                .and_then(|dimension| dimension.measurement_id);
            if measurement_id.is_some_and(|id| zone.measurement_ids.contains(&id)) {
                zone.dimension_indices.push(*dimension_index);
            }
        }
        split.push(zone);
    }
    let unmatched = group.dimension_indices.into_iter().filter(|dimension_index| {
        let measurement_id = photo.reference.as_ref().and_then(|reference| reference.dimensions.get(*dimension_index))
            .and_then(|dimension| dimension.measurement_id);
        measurement_id.is_none_or(|id| !group.measurement_ids.contains(&id))
    }).collect::<Vec<_>>();
    if !unmatched.is_empty() {
        split.push(ReportGroup {
            description: format!("{} · cotes sans mesure associée", group.description),
            dimension_indices: unmatched,
            ..ReportGroup::default()
        });
    }
    split
}

fn split_measurements_spatially(mut measurements: Vec<&Measurement>, maximum: usize) -> Vec<Vec<u64>> {
    if measurements.len() <= maximum { return vec![measurements.iter().map(|measurement| measurement.id).collect()]; }
    let centers = measurements.iter().map(|measurement| measurement_center(measurement)).collect::<Vec<_>>();
    let min_x = centers.iter().map(|point| point.0).fold(f64::INFINITY, f64::min);
    let max_x = centers.iter().map(|point| point.0).fold(f64::NEG_INFINITY, f64::max);
    let min_y = centers.iter().map(|point| point.1).fold(f64::INFINITY, f64::min);
    let max_y = centers.iter().map(|point| point.1).fold(f64::NEG_INFINITY, f64::max);
    let split_x = max_x - min_x >= max_y - min_y;
    measurements.sort_by(|left, right| {
        let a = measurement_center(left);
        let b = measurement_center(right);
        let primary = if split_x { a.0.total_cmp(&b.0) } else { a.1.total_cmp(&b.1) };
        let secondary = if split_x { a.1.total_cmp(&b.1) } else { a.0.total_cmp(&b.0) };
        primary.then(secondary).then_with(|| left.id.cmp(&right.id))
    });
    let right = measurements.split_off(measurements.len() / 2);
    let mut zones = split_measurements_spatially(measurements, maximum);
    zones.extend(split_measurements_spatially(right, maximum));
    zones
}

fn measurement_center(measurement: &Measurement) -> (f64, f64) {
    if measurement.points.is_empty() { return (0.0, 0.0); }
    let count = measurement.points.len() as f64;
    (measurement.points.iter().map(|point| point.x).sum::<f64>() / count,
        measurement.points.iter().map(|point| point.y).sum::<f64>() / count)
}

fn report_group_center(group: &ReportGroup, photo: &FolderReportPhoto) -> (f64, f64) {
    let centers = photo.session.measurements.iter().filter(|measurement| group.measurement_ids.contains(&measurement.id))
        .map(measurement_center).collect::<Vec<_>>();
    if centers.is_empty() { return (0.0, 0.0); }
    (centers.iter().map(|center| center.0).sum::<f64>() / centers.len() as f64,
        centers.iter().map(|center| center.1).sum::<f64>() / centers.len() as f64)
}

fn annotation_anchor(annotation: &crate::geometry::Annotation, measurements: &[Measurement]) -> Option<Point> {
    match &annotation.target {
        crate::geometry::AnnotationTarget::Measurement { measurement_id } => measurements.iter()
            .find(|measurement| measurement.id == *measurement_id)
            .map(|measurement| { let center = measurement_center(measurement); Point { x: center.0, y: center.1 } }),
        crate::geometry::AnnotationTarget::Point { point } => Some(*point),
        crate::geometry::AnnotationTarget::Zone { first, second } =>
            Some(Point { x: (first.x + second.x) / 2.0, y: (first.y + second.y) / 2.0 }),
    }
}

#[derive(Clone)]
struct GroupComparisonRow {
    cells: [String; 6],
    status_color: &'static str,
    measurement_color: &'static str,
}

fn group_comparison_rows(photo: &FolderReportPhoto, group: &ReportGroup) -> Vec<GroupComparisonRow> {
    let mut rows = Vec::new();
    let mut represented = Vec::<u64>::new();
    if let Some(reference) = &photo.reference {
        for dimension_index in &group.dimension_indices {
            let Some(dimension) = reference.dimensions.get(*dimension_index) else { continue; };
            let measurement = dimension.measurement_id.and_then(|id| {
                photo.session.measurements.iter().find(|measurement| measurement.id == id)
            });
            if let Some(measurement) = measurement { push_unique(&mut represented, measurement.id); }
            let id_label = measurement.map(|measurement| format!("M{}", measurement.id))
                .or_else(|| dimension.measurement_id.map(|id| format!("M{id}")));
            let label = match id_label {
                Some(id) if !dimension.label.trim().is_empty() => format!("{id} · {}", dimension.label.trim()),
                Some(id) => id,
                None if !dimension.label.trim().is_empty() => dimension.label.trim().to_owned(),
                None => "Cote de référence".to_owned(),
            };
            let source_label = dimension.id.strip_prefix("ocr-")
                .and_then(|index| index.parse::<usize>().ok())
                .map(|index| {
                    let page = dimension.ocr_page.map(|page| format!(" · p. {page}")).unwrap_or_default();
                    let confidence = dimension.ocr_confidence
                        .map(|value| format!(" · {value:.0}%"))
                        .unwrap_or_default();
                    format!("OCR {index}{page}{confidence}")
                })
                .or_else(|| dimension.id.strip_prefix("pdf-text-").and_then(|_| dimension.ocr_page)
                    .map(|page| format!("PDF texte · p. {page}")));
            let label = source_label.map(|source| format!("{source} · {label}")).unwrap_or(label);
            let measured_value = dimension.measured_mm.or_else(|| measurement
                .and_then(|measurement| numeric_result_mm(&geometry::result(measurement))));
            let uncertainty = dimension.expanded_uncertainty_mm.or_else(|| {
                measurement.and_then(|measurement| measurement.expanded_uncertainty_mm)
            });
            let measured = measured_value.map(|value| uncertainty
                .map(|u| format!("{value:.3} ± {u:.3} mm"))
                .unwrap_or_else(|| format!("{value:.3} mm · U inconnue")))
                .or_else(|| measurement.map(geometry::result)).unwrap_or_else(|| "—".to_owned());
            let deviation = measured_value.map(|value| format!("{:+.3} mm", value - dimension.nominal_mm))
                .unwrap_or_else(|| "—".to_owned());
            let status = if measurement.is_none() {
                "Non associée · à confirmer"
            } else if crate::report::is_vision_measurement(&photo.session, dimension.measurement_id) {
                "Verdict suspendu"
            } else if reference.verdict_notice.is_some()
                && matches!(dimension.status.as_str(), "Dans la tolérance" | "Hors tolérance") {
                "Verdict suspendu"
            } else {
                dimension.status.as_str()
            };
            rows.push(make_group_row(
                &label,
                Some(dimension.nominal_mm),
                Some(dimension.tolerance_minus_mm),
                Some(dimension.tolerance_plus_mm),
                &measured,
                &deviation,
                status,
                measurement.map(|measurement| measurement.id),
            ));
        }
    }
    for measurement in photo.session.measurements.iter()
        .filter(|measurement| group.measurement_ids.contains(&measurement.id))
    {
        if represented.contains(&measurement.id) { continue; }
        let label = if measurement.label.trim().is_empty() || measurement.label == format!("M{}", measurement.id) {
            format!("M{}", measurement.id)
        } else {
            format!("M{} · {}", measurement.id, measurement.label.trim())
        };
        let result = geometry::result(measurement);
        let measured = if let Some(value) = numeric_result_mm(&result) {
            measurement.expanded_uncertainty_mm.map(|u| format!("{value:.3} ± {u:.3} mm"))
                .unwrap_or_else(|| format!("{value:.3} mm · U inconnue"))
        } else { result };
        rows.push(make_group_row(
            &label,
            None,
            None,
            None,
            &measured,
            "—",
            if measurement.origin.eq_ignore_ascii_case("vision") { "Verdict suspendu" }
            else if photo.reference.is_some() { "Non associée" } else { "Pas de référence" },
            Some(measurement.id),
        ));
    }
    rows
}

fn make_group_row(
    label: &str, nominal: Option<f64>, tolerance_minus: Option<f64>, tolerance_plus: Option<f64>,
    measured: &str, deviation: &str, status: &str, measurement_id: Option<u64>,
) -> GroupComparisonRow {
    let nominal = nominal.map(|value| format!("{value:.3} mm")).unwrap_or_else(|| "—".to_owned());
    let tolerance = match (tolerance_minus, tolerance_plus) {
        (Some(minus), Some(plus)) => format!("-{minus:.3}/+{plus:.3} mm"),
        _ => "—".to_owned(),
    };
    let status_color = match status {
        "Dans la tolérance" => "20804A",
        "Hors tolérance" => "B42318",
        "Non associée" | "Non associée · à confirmer" => "63738A",
        _ => "8A5A10",
    };
    let measurement_color = measurement_id.map(|id| PALETTE[(id.saturating_sub(1) as usize) % PALETTE.len()])
        .unwrap_or("63738A");
    GroupComparisonRow {
        cells: [label.to_owned(), nominal, tolerance, measured.to_owned(), deviation.to_owned(), status.to_owned()],
        status_color,
        measurement_color,
    }
}

fn numeric_result_mm(result: &str) -> Option<f64> {
    let value = result.strip_suffix(" mm")?.trim().parse::<f64>().ok()?;
    value.is_finite().then_some(value)
}

fn split_status(rows: &[GroupComparisonRow], photo: &FolderReportPhoto) -> String {
    let statuses = rows.iter().map(|row| row.cells[5].as_str()).collect::<Vec<_>>();
    if statuses.iter().any(|status| *status == "Verdict suspendu") { return "Verdict suspendu".to_owned(); }
    if statuses.iter().any(|status| *status == "Hors tolérance") { return "Hors tolérance".to_owned(); }
    if photo.reference.as_ref().is_some_and(|reference| reference.verdict_notice.is_some())
        && statuses.iter().any(|status| *status == "Dans la tolérance") {
        return "Verdict suspendu".to_owned();
    }
    if statuses.iter().all(|status| *status == "Dans la tolérance") && !statuses.is_empty() {
        return "Dans la tolérance".to_owned();
    }
    if statuses.iter().any(|status| *status == "À confirmer" || *status == "Non associée · à confirmer") {
        return "À confirmer".to_owned();
    }
    if statuses.is_empty() { "Sans mesure associée".to_owned() } else { "À confirmer".to_owned() }
}

fn group_table_shapes(rows: &[GroupComparisonRow], first_row: usize, row_count: usize, top: i64, next_id: &mut u32) -> String {
    let mut xml = String::new();
    let headings = ["Mesure / cote", "Nominal", "Tolérances", "Mesuré ± U", "Écart", "Statut"];
    let mut x = TABLE_X;
    let header_height = 290_000;
    for (column, heading) in headings.iter().enumerate() {
        xml.push_str(&text_shape(*next_id, "En-tête comparaison par groupe", x, top, GROUP_TABLE_COLS[column], header_height,
            heading, "FFFFFF", 9, true, Some("17385E")));
        *next_id += 1;
        x += GROUP_TABLE_COLS[column];
    }
    let row_height = 275_000;
    if rows.is_empty() {
        xml.push_str(&text_shape(*next_id, "Groupe sans ligne de mesure", TABLE_X, top + header_height, TABLE_W, row_height,
            "Aucune mesure ni cote de référence dans ce groupe.", "52657F", 9, false, Some("F8FAFC")));
        *next_id += 1;
        return xml;
    }
    for (offset, row) in rows.iter().enumerate().skip(first_row).take(row_count) {
        let y = top + header_height + (offset - first_row) as i64 * row_height;
        let shade = if (offset - first_row) % 2 == 0 { "F8FAFC" } else { "FFFFFF" };
        let mut x = TABLE_X;
        for column in 0..6 {
            let color = match column {
                0 => row.measurement_color,
                5 => row.status_color,
                _ => "273F5E",
            };
            xml.push_str(&text_shape(*next_id, "Ligne de comparaison par groupe", x, y, GROUP_TABLE_COLS[column], row_height,
                &row.cells[column], color, if column == 0 || column == 5 { 8 } else { 7 }, column == 0 || column == 5,
                Some(shade)));
            *next_id += 1;
            x += GROUP_TABLE_COLS[column];
        }
    }
    xml
}

fn append_report_context(
    body: &mut String, options: &FolderReportOptions, photo: &FolderReportPhoto,
    spec: &SlideSpec, photo_total: usize, mut y: i64, next_id: &mut u32,
) -> i64 {
    let mut summary = Vec::new();
    if options.include_folder_name && !options.folder_name.trim().is_empty() { summary.push(options.folder_name.clone()); }
    if options.include_picture_count { summary.push(format!("{photo_total} image(s)")); }
    if options.include_generation_date { summary.push(format!("Généré le {}", crate::report::utc_timestamp_for_export())); }
    if !summary.is_empty() {
        body.push_str(&text_shape(*next_id, "Contexte rapport", 410_000, y, 11_300_000, 185_000,
            &summary.join(" · "), "52657F", 8, false, None));
        *next_id += 1;
        y += 200_000;
    }
    let mut heading = Vec::new();
    if options.include_sequence_numbers { heading.push(format!("Image {} / {}", spec.photo_number, photo_total)); }
    if options.include_file_names { heading.push(photo.name.clone()); }
    if !heading.is_empty() {
        body.push_str(&text_shape(*next_id, "Photo rapport", 410_000, y, 11_300_000, 185_000,
            &heading.join(" · "), "273F5E", 9, true, None));
        *next_id += 1;
        y += 200_000;
    }
    if options.include_descriptions && !photo.description.trim().is_empty() {
        body.push_str(&text_shape(*next_id, "Description photo", 410_000, y, 11_300_000, 185_000,
            &photo.description, "52657F", 8, false, None));
        *next_id += 1;
        y += 200_000;
    }
    if let Some(warning) = photo.warning.as_deref().filter(|value| !value.trim().is_empty()) {
        body.push_str(&text_shape(*next_id, "Avertissement photo", 410_000, y, 11_300_000, 185_000,
            warning, "B42318", 8, true, Some("FEF3F2")));
        *next_id += 1;
        y += 200_000;
    }
    if photo.include_timestamp {
        if let Some(timestamp) = photo.timestamp.as_deref() {
            body.push_str(&text_shape(*next_id, "Horodatage photo", 410_000, y, 11_300_000, 185_000,
                timestamp, "52657F", 8, false, None));
            *next_id += 1;
            y += 200_000;
        }
    }
    y
}

fn group_overview_slide_xml(
    options: &FolderReportOptions, photo: &FolderReportPhoto, photo_total: usize, spec: &SlideSpec,
    groups: &[NumberedReportGroup], first_group: usize, group_count: usize,
    slide_index: usize, slide_total: usize,
) -> String {
    let mut body = slide_tree_start();
    let mut next_id = 2u32;
    let mut y = 255_000i64;
    if options.include_title && !options.title.trim().is_empty() {
        body.push_str(&text_shape(next_id, "Titre du rapport", 410_000, y, 11_300_000, 310_000,
            &options.title, "1F4E8C", 21, true, None));
        next_id += 1;
        y += 335_000;
    }
    y = append_report_context(&mut body, options, photo, spec, photo_total, y, &mut next_id);
    let heading = "Vue générale des groupes";
    body.push_str(&text_shape(next_id, "Index localisateur des groupes", 410_000, y, 11_300_000, 270_000,
        heading, "273F5E", 13, true, None));
    next_id += 1;
    y += 285_000;
    if let Some(notice) = photo.series_notice.as_deref() {
        body.push_str(&text_shape(next_id, "Avis de comparabilité", 410_000, y, 11_300_000, 200_000,
            notice, "8A5A10", 8, false, Some("FFF7E0")));
        next_id += 1;
        y += 215_000;
    }
    let image_top = (y + 70_000).clamp(1_160_000, 2_000_000);
    let image_box = fit_image(photo.width, photo.height, 410_000, image_top, 7_100_000, 6_250_000 - image_top);
    body.push_str(&picture_shape(next_id, image_box, "rId2"));
    next_id += 1;
    for item in groups.iter().skip(first_group).take(group_count) {
        let measurements = photo.session.measurements.iter()
            .filter(|measurement| item.group.measurement_ids.contains(&measurement.id)).collect::<Vec<_>>();
        if measurements.is_empty() { continue; }
        let color = PALETTE[(item.number.saturating_sub(1)) % PALETTE.len()];
        let centers = measurements.iter().map(|measurement| measurement_center(measurement)).collect::<Vec<_>>();
        let min_x = centers.iter().map(|point| point.0).fold(f64::INFINITY, f64::min);
        let max_x = centers.iter().map(|point| point.0).fold(f64::NEG_INFINITY, f64::max);
        let min_y = centers.iter().map(|point| point.1).fold(f64::INFINITY, f64::min);
        let max_y = centers.iter().map(|point| point.1).fold(f64::NEG_INFINITY, f64::max);
        let a = map_point(Point { x: min_x, y: min_y }, image_box, photo.width, photo.height);
        let b = map_point(Point { x: max_x, y: max_y }, image_box, photo.width, photo.height);
        let pad = 90_000;
        let left = (a.0.min(b.0) - pad).max(image_box.0);
        let right = (a.0.max(b.0) + pad).min(image_box.0 + image_box.2);
        let top = (a.1.min(b.1) - pad).max(image_box.1);
        let bottom = (a.1.max(b.1) + pad).min(image_box.1 + image_box.3);
        let corners = [(left, top), (right.max(left + 1), top), (right.max(left + 1), bottom.max(top + 1)), (left, bottom.max(top + 1))];
        body.push_str(&path_shape(next_id, &corners, true, color));
        next_id += 1;
        let center = report_group_center(&item.group, photo);
        let marker = map_point(Point { x: center.0, y: center.1 }, image_box, photo.width, photo.height);
        body.push_str(&ellipse_shape(next_id, marker.0 - 55_000, marker.1 - 55_000, 110_000, 110_000, color));
        next_id += 1;
        let description = report::short_group_marker_characteristic(&item.group.description);
        let label_width = image_box.2.min(1_750_000);
        let label_x = (marker.0 + 65_000).min((image_box.0 + image_box.2 - label_width).max(image_box.0));
        let label_y = (top + 15_000).clamp(image_box.1, image_box.1 + image_box.3 - 220_000);
        body.push_str(&text_shape(next_id, "Repère du groupe", label_x, label_y, label_width, 210_000,
            &format!("Groupe {} · {}", item.number, description), "FFFFFF", 8, true, Some(color)));
        next_id += 1;
    }
    let panel_x = 7_720_000;
    let row_top = image_top;
    body.push_str(&text_shape(next_id, "Légende groupes", panel_x, row_top, 4_060_000, 300_000,
        "Index des groupes", "1F4E8C", 13, true, Some("E7EEF7")));
    next_id += 1;
    body.push_str(&text_shape(next_id, "Règle de groupement", panel_x, row_top + 310_000, 4_060_000, 455_000,
        "Classement par type/caractéristique. Au-delà de 6 mesures : coupures récursives par moitiés\nsur l’axe X/Y le plus dispersé des centres.",
        "43536D", 7, false, None));
    next_id += 1;
    for (offset, item) in groups.iter().enumerate().skip(first_group).take(group_count) {
        let index = offset - first_group;
        let y = row_top + 800_000 + index as i64 * 385_000;
        let count = if item.group.measurement_ids.is_empty() {
            format!("{} cote(s), sans mesure associée", item.group.dimension_indices.len())
        } else {
            format!("{} mesure(s) · {} cote(s)", item.group.measurement_ids.len(), item.group.dimension_indices.len())
        };
        let color = PALETTE[(item.number.saturating_sub(1)) % PALETTE.len()];
        body.push_str(&text_shape(next_id, "Entrée d’index groupe", panel_x, y, 4_060_000, 360_000,
            &format!("Groupe {} · {}\n{}", item.number, item.group.description, count), color, 9, true, Some("FFFFFF")));
        next_id += 1;
    }
    body.push_str(&pptx_footer(slide_index, slide_total));
    slide_tree_end(body)
}

fn group_slide_xml(
    options: &FolderReportOptions, photo: &FolderReportPhoto, photo_total: usize, spec: &SlideSpec,
    item: &NumberedReportGroup, first_row: usize, row_count: usize,
    slide_index: usize, slide_total: usize,
) -> String {
    let rows = group_comparison_rows(photo, &item.group);
    let mut body = slide_tree_start();
    let mut next_id = 2u32;
    let title = if options.include_title && !options.title.trim().is_empty() {
        format!("{} · Groupe {} · {}", options.title.trim(), item.number, item.group.description)
    } else {
        format!("Groupe {} · {}", item.number, item.group.description)
    };
    body.push_str(&text_shape(next_id, "Titre groupe", 410_000, 250_000, 11_300_000, 330_000,
        &title, "1F4E8C", 17, true, None));
    next_id += 1;
    let status = split_status(&rows, photo);
    let mut y = append_report_context(&mut body, options, photo, spec, photo_total, 600_000, &mut next_id);
    let group_summary = format!("{} mesure(s) · statut du groupe : {}", item.group.measurement_ids.len(), status);
    body.push_str(&text_shape(next_id, "Résumé groupe", 410_000, y, 11_300_000, 200_000,
        &group_summary, "52657F", 9, false, None));
    next_id += 1;
    y += 210_000;
    if let Some(notice) = photo.reference.as_ref().and_then(|reference| reference.verdict_notice.as_deref())
        .or(photo.series_notice.as_deref()) {
        body.push_str(&text_shape(next_id, "Avis de référence", 410_000, y, 11_300_000, 180_000,
            notice, "8A5A10", 8, false, Some("FFF7E0")));
        next_id += 1;
        y += 190_000;
    }
    let image_top = (y + 40_000).clamp(1_080_000, 2_150_000);
    let image_box = fit_image(photo.width, photo.height, 410_000, image_top, 6_650_000, (4_000_000 - image_top).max(1_200_000));
    body.push_str(&picture_shape(next_id, image_box, "rId2"));
    next_id += 1;
    let group_label = format!("Groupe {} · {}", item.number, item.group.description);
    body.push_str(&text_shape(next_id, "Caractéristique groupe sur image", image_box.0, image_box.1, image_box.2.min(3_100_000), 220_000,
        &group_label, "FFFFFF", 9, true, Some(PALETTE[(item.number.saturating_sub(1)) % PALETTE.len()])));
    next_id += 1;
    for measurement in photo.session.measurements.iter()
        .filter(|measurement| item.group.measurement_ids.contains(&measurement.id))
    {
        body.push_str(&measurement_vectors(measurement, image_box, photo.width, photo.height, &mut next_id));
    }
    let mut occupied_annotation_badges = Vec::<(i64, i64, i64, i64)>::new();
    let mut measurement_vector_obstacles = vec![(
        image_box.0,
        image_box.1,
        image_box.0 + image_box.2.min(3_100_000),
        image_box.1 + 220_000,
    )];
    measurement_vector_obstacles.extend(photo.session.measurements.iter()
        .filter(|measurement| item.group.measurement_ids.contains(&measurement.id))
        .flat_map(|measurement| {
            let points = geometry::sampled_curve(measurement.kind, &measurement.points, 128)
                .unwrap_or_else(|| measurement.points.clone());
            let mut obstacles = points.windows(2).map(|segment| {
                let first = map_point(segment[0], image_box, photo.width, photo.height);
                let second = map_point(segment[1], image_box, photo.width, photo.height);
                let padding = 35_000i64;
                (first.0.min(second.0) - padding, first.1.min(second.1) - padding,
                    first.0.max(second.0) + padding, first.1.max(second.1) + padding)
            }).collect::<Vec<_>>();
            if obstacles.is_empty() {
                obstacles.extend(points.iter().map(|point| {
                    let mapped = map_point(*point, image_box, photo.width, photo.height);
                    (mapped.0 - 35_000, mapped.1 - 35_000, mapped.0 + 35_000, mapped.1 + 35_000)
                }));
            }
            if let Some((x, y)) = measurement.points.last()
                .map(|point| map_point(*point, image_box, photo.width, photo.height))
            {
                let label_width = 510_000;
                let label_height = 180_000;
                let label_x = (x + 36_000).max(image_box.0)
                    .min((image_box.0 + image_box.2 - label_width).max(image_box.0));
                let label_y = (y - 120_000).max(image_box.1)
                    .min((image_box.1 + image_box.3 - label_height).max(image_box.1));
                let padding = 20_000;
                obstacles.push((label_x - padding, label_y - padding,
                    label_x + label_width + padding, label_y + label_height + padding));
            }
            obstacles
        })
        .collect::<Vec<_>>());
    for annotation_index in &item.group.annotation_indices {
        let Some(annotation) = photo.session.annotations.get(*annotation_index) else { continue; };
        let Some(anchor) = annotation_anchor(annotation, &photo.session.measurements) else { continue; };
        let point = map_point(anchor, image_box, photo.width, photo.height);
        let color = PALETTE[annotation_index % PALETTE.len()];
        body.push_str(&ellipse_shape(next_id, point.0 - 45_000, point.1 - 45_000, 90_000, 90_000, color));
        next_id += 1;
        let label = format!("A{}", annotation_index + 1);
        let badge_width = (label.len() as i64 * 72_000).max(210_000);
        let badge_height = 170_000i64;
        let mut safe_badge = None;
        'rings: for radius in [140_000.0_f64, 230_000.0, 350_000.0, 500_000.0, 680_000.0] {
            for direction in 0..16 {
                let angle = direction as f64 * std::f64::consts::TAU / 16.0;
                let center_x = point.0 + (angle.cos() * radius).round() as i64;
                let center_y = point.1 + (angle.sin() * radius).round() as i64;
                let candidate = (center_x - badge_width / 2, center_y - badge_height / 2,
                    center_x + badge_width / 2, center_y + badge_height / 2);
                if candidate.0 < image_box.0 || candidate.1 < image_box.1
                    || candidate.2 > image_box.0 + image_box.2
                    || candidate.3 > image_box.1 + image_box.3
                    || measurement_vector_obstacles.iter().any(|obstacle| rectangles_overlap(*obstacle, candidate))
                    || occupied_annotation_badges.iter().any(|occupied| rectangles_overlap(*occupied, candidate))
                {
                    continue;
                }
                safe_badge = Some(candidate);
                break 'rings;
            }
        }
        if let Some((left, top, right, bottom)) = safe_badge {
            let center = ((left + right) / 2, (top + bottom) / 2);
            body.push_str(&path_shape(next_id, &[point, center], false, color));
            next_id += 1;
            body.push_str(&text_shape(next_id, "Repère annotation", left, top, right - left, bottom - top,
                &label, "FFFFFF", 8, true, Some(color)));
            next_id += 1;
            occupied_annotation_badges.push((left - 25_000, top - 25_000, right + 25_000, bottom + 25_000));
        }
    }
    let panel_x = 7_250_000;
    body.push_str(&text_shape(next_id, "Annotations du groupe", panel_x, 1_080_000, 4_060_000, 300_000,
        "Annotations", "1F4E8C", 11, true, Some("E7EEF7")));
    next_id += 1;
    if item.group.annotation_indices.is_empty() {
        body.push_str(&text_shape(next_id, "Aucune annotation groupe", panel_x, 1_410_000, 4_060_000, 330_000,
            "Aucune annotation associée à ce groupe.", "63738A", 9, false, Some("FFFFFF")));
        next_id += 1;
    } else {
        for (index, annotation_index) in item.group.annotation_indices.iter().take(6).enumerate() {
            let Some(annotation) = photo.session.annotations.get(*annotation_index) else { continue; };
            let y = 1_410_000 + index as i64 * 365_000;
            body.push_str(&text_shape(next_id, "Texte annotation", panel_x, y, 4_060_000, 335_000,
                &format!("A{} · {}", annotation_index + 1, annotation.text), "273F5E", 8, false, Some("FFFFFF")));
            next_id += 1;
        }
        if item.group.annotation_indices.len() > 6 {
            body.push_str(&text_shape(next_id, "Annotations supplémentaires", panel_x, 3_650_000, 4_060_000, 250_000,
                &format!("Commentaires complets : {} note(s) dans les diapositives dédiées", item.group.annotation_indices.len()),
                "63738A", 8, false, None));
            next_id += 1;
        }
    }
    if first_row == 0 && rows.is_empty() {
        body.push_str(&group_table_shapes(&rows, 0, 0, 4_100_000, &mut next_id));
    } else {
        body.push_str(&group_table_shapes(&rows, first_row, row_count, 4_100_000, &mut next_id));
    }
    body.push_str(&pptx_footer(slide_index, slide_total));
    slide_tree_end(body)
}

fn group_annotations_slide_xml(
    options: &FolderReportOptions,
    photo: &FolderReportPhoto,
    photo_total: usize,
    spec: &SlideSpec,
    item: &NumberedReportGroup,
    first_annotation: usize,
    annotation_count: usize,
    slide_index: usize,
    slide_total: usize,
) -> String {
    let mut body = slide_tree_start();
    let mut next_id = 2u32;
    let mut y = 255_000i64;
    if options.include_title && !options.title.trim().is_empty() {
        body.push_str(&text_shape(next_id, "Titre du rapport", 410_000, y, 11_300_000, 310_000,
            &options.title, "1F4E8C", 20, true, None));
        next_id += 1;
        y += 335_000;
    }
    y = append_report_context(&mut body, options, photo, spec, photo_total, y, &mut next_id);
    let total_pages = item.group.annotation_indices.len().div_ceil(4).max(1);
    let page_number = first_annotation / 4 + 1;
    let heading = format!("Commentaires · Groupe {} · page {} / {}", item.number, page_number, total_pages);
    body.push_str(&text_shape(next_id, "Titre des commentaires du groupe", 410_000, y, 11_300_000, 300_000,
        &heading, "1F4E8C", 14, true, None));
    next_id += 1;
    y += 360_000;
    body.push_str(&text_shape(next_id, "Règle du groupe", 410_000, y, 11_300_000, 260_000,
        &format!("{} · commentaires complets liés à leur cible", item.group.description),
        "52657F", 9, false, None));
    next_id += 1;
    y += 320_000;

    const ANNOTATION_ROW_HEIGHT: i64 = 1_180_000;
    for (offset, annotation_index) in item.group.annotation_indices.iter()
        .enumerate().skip(first_annotation).take(annotation_count)
    {
        let Some(annotation) = photo.session.annotations.get(*annotation_index) else { continue; };
        let target = pptx_annotation_target_description(annotation, &photo.session.measurements);
        let note = if annotation.text.trim().is_empty() { "(commentaire vide)" } else { annotation.text.trim() };
        let text = format!("A{} · {}\n{}", annotation_index + 1, target, note);
        let y = y + (offset - first_annotation) as i64 * ANNOTATION_ROW_HEIGHT;
        body.push_str(&text_shape(next_id, "Commentaire complet du groupe", 410_000, y,
            11_300_000, ANNOTATION_ROW_HEIGHT - 40_000, &text, "273F5E", 10, false,
            Some(if offset % 2 == 0 { "F8FAFC" } else { "FFFFFF" })));
        next_id += 1;
    }

    body.push_str(&pptx_footer(slide_index, slide_total));
    slide_tree_end(body)
}

fn pptx_annotation_target_description(
    annotation: &crate::geometry::Annotation,
    measurements: &[Measurement],
) -> String {
    match &annotation.target {
        crate::geometry::AnnotationTarget::Measurement { measurement_id } => measurements.iter()
            .find(|measurement| measurement.id == *measurement_id)
            .map(|measurement| {
                if measurement.label.trim().is_empty() {
                    format!("Mesure M{} · {}", measurement.id, measurement.kind.label())
                } else {
                    format!("Mesure M{} · {}", measurement.id, measurement.label.trim())
                }
            })
            .unwrap_or_else(|| format!("Mesure M{measurement_id}")),
        crate::geometry::AnnotationTarget::Point { point } =>
            format!("Point ({:.1}; {:.1}) px", point.x, point.y),
        crate::geometry::AnnotationTarget::Zone { first, second } => format!(
            "Zone ({:.1}; {:.1})–({:.1}; {:.1}) px", first.x, first.y, second.x, second.y,
        ),
    }
}

fn build_audit_lines(options: &FolderReportOptions, photos: &[FolderReportPhoto]) -> Vec<AuditLine> {
    let inspection_lines = photos.iter().flat_map(|photo| crate::report::inspection_export_notes(&photo.session)
        .into_iter().map(move |value| AuditLine { label: format!("Inspection · {}", photo.name), value }))
        .collect::<Vec<_>>();
    if !options.include_audit_page {
        return expand_audit_lines(inspection_lines);
    }
    let mut lines = vec![
        AuditLine { label: "Opérateur".into(), value: options.operator_name.clone() },
        AuditLine { label: "Référence pièce".into(), value: options.part_reference.clone() },
        AuditLine { label: "Révision du plan".into(), value: options.drawing_revision.clone() },
        AuditLine { label: "Lot".into(), value: options.lot_number.clone() },
        AuditLine { label: "Décision".into(), value: options.approval_status.clone() },
        AuditLine { label: "Validé par".into(), value: options.approved_by.clone() },
        AuditLine { label: "Généré le".into(), value: crate::report::utc_timestamp_for_export() },
        AuditLine { label: "Images sélectionnées".into(), value: photos.len().to_string() },
    ];
    if !options.audit_notes.trim().is_empty() {
        lines.push(AuditLine { label: "Notes d’inspection".into(), value: options.audit_notes.trim().to_owned() });
    }
    lines.extend(inspection_lines);
    let mut corrections = Vec::new();
    for photo in photos {
        lines.push(AuditLine { label: format!("Acquisition · {}", photo.name),
            value: format!("{} × {} px · appareil {} [{}] · optique {}", photo.width, photo.height,
                photo.session.device, photo.session.device_id, photo.session.optical_setup) });
        let calibration = photo.session.calibration.as_ref().map(|profile| format!(
            "{} · {} · X {:.8} / Y {:.8} mm/px · U {}",
            profile.id, profile.validation_status,
            profile.scale_x_mm_per_pixel.unwrap_or(profile.mm_per_pixel),
            profile.scale_y_mm_per_pixel.unwrap_or(profile.mm_per_pixel),
            profile.expanded_uncertainty_mm.map(|value| format!("{value:.8} mm")).unwrap_or_else(|| "inconnue".into())))
            .unwrap_or_else(|| "Non étalonné".into());
        lines.push(AuditLine { label: "Calibration".into(), value: calibration });
        for event in &photo.session.measurement_audit_history {
            corrections.push((event.timestamp_unix_secs, photo.name.clone(), event.action.clone(),
                event.measurement_ids.iter().map(|id| format!("M{id}")).collect::<Vec<_>>().join(", "),
                event.before.iter().map(|value| format!("M{} {}={}", value.id, value.label, value.result)).collect::<Vec<_>>().join("; "),
                event.after.iter().map(|value| format!("M{} {}={}", value.id, value.label, value.result)).collect::<Vec<_>>().join("; ")));
        }
    }
    corrections.sort_by_key(|event| event.0);
    for (timestamp, photo, action, ids, before, after) in corrections {
        lines.push(AuditLine { label: "Historique des corrections".into(), value: format!(
            "{} · {} · {} · {} · avant [{}] · après [{}]", crate::report::utc_timestamp_at(timestamp), photo, action, ids, before, after) });
    }
    let mut expanded = Vec::new();
    for line in lines {
        let mut chunks = Vec::new();
        let mut current = String::new();
        for word in line.value.split_whitespace() {
            if !current.is_empty() && current.chars().count() + word.chars().count() + 1 > 150 {
                chunks.push(std::mem::take(&mut current));
            }
            if !current.is_empty() { current.push(' '); }
            current.push_str(word);
        }
        if !current.is_empty() || chunks.is_empty() { chunks.push(current); }
        for (index, value) in chunks.into_iter().enumerate() {
            expanded.push(AuditLine {
                label: if index == 0 { line.label.clone() } else { format!("{} · suite", line.label) },
                value,
            });
        }
    }
    expanded
}

fn expand_audit_lines(lines: Vec<AuditLine>) -> Vec<AuditLine> {
    let mut expanded = Vec::new();
    for line in lines {
        let mut chunks = Vec::new();
        let mut current = String::new();
        for word in line.value.split_whitespace() {
            if !current.is_empty() && current.chars().count() + word.chars().count() + 1 > 150 {
                chunks.push(std::mem::take(&mut current));
            }
            if !current.is_empty() { current.push(' '); }
            current.push_str(word);
        }
        if !current.is_empty() || chunks.is_empty() { chunks.push(current); }
        for (index, value) in chunks.into_iter().enumerate() {
            expanded.push(AuditLine {
                label: if index == 0 { line.label.clone() } else { format!("{} · suite", line.label) },
                value,
            });
        }
    }
    expanded
}

fn audit_slide_xml(options: &FolderReportOptions, lines: &[AuditLine], first_line: usize, line_count: usize,
    slide_index: usize, slide_total: usize) -> String {
    let mut body = slide_tree_start();
    let title = if options.include_title && !options.title.trim().is_empty() {
        format!("{} · FICHE DE CONTRÔLE ET D’AUDIT", options.title.trim())
    } else { "FICHE DE CONTRÔLE ET D’AUDIT".to_owned() };
    body.push_str(&text_shape(2, "Fiche audit", 410_000, 270_000, 11_300_000, 400_000,
        &title,
        "1F4E8C", 18, true, None));
    let page_count = lines.len().max(1).div_ceil(28);
    if page_count > 1 {
        body.push_str(&text_shape(3, "Pagination audit", 410_000, 680_000, 11_300_000, 220_000,
            &format!("Suite de la fiche d’audit · page {} / {}", first_line / 28 + 1, page_count), "52657F", 9, false, None));
    }
    for (index, line) in lines.iter().enumerate().skip(first_line).take(line_count) {
        let y = 900_000 + (index - first_line) as i64 * 195_000;
        body.push_str(&text_shape((index - first_line) as u32 + 4, "Ligne fiche audit", 410_000, y, 2_100_000, 185_000,
            &line.label, "273F5E", 8, true, Some("E7EEF7")));
        body.push_str(&text_shape((index - first_line) as u32 + 4 + line_count as u32, "Valeur fiche audit", 2_560_000, y, 9_150_000, 185_000,
            &line.value, "273F5E", 8, false, Some("FFFFFF")));
    }
    body.push_str(&pptx_footer(slide_index, slide_total));
    slide_tree_end(body)
}

fn slide_tree_start() -> String {
    "<p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>".into()
}

fn pptx_footer(slide_index: usize, slide_total: usize) -> String {
    text_shape(u32::MAX - 1, "Pied de page DPER_Measure", 410_000, 6_605_000, 11_300_000, 150_000,
        &format!("DPER_Measure · Diapositive {} / {} · U élargie requise pour un verdict", slide_index + 1, slide_total),
        "63738A", 8, false, None)
}

fn slide_tree_end(mut body: String) -> String {
    body.push_str("</p:spTree>");
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:sld xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:cSld><p:bg><p:bgPr><a:solidFill><a:srgbClr val=\"F4F7FB\"/></a:solidFill><a:effectLst/></p:bgPr></p:bg>{body}</p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>")
}

fn slide_xml(options: &FolderReportOptions, photos: &[FolderReportPhoto], spec: &SlideSpec, slide_index: usize, slide_total: usize) -> String {
    let photo = &photos[spec.photo];
    match &spec.content {
        SlideContent::Reference { page, first_row, row_count } =>
            return reference_slide_xml(photo, *page, *first_row, *row_count, slide_index, slide_total),
        SlideContent::GroupOverview { groups, first_group, group_count } =>
            return group_overview_slide_xml(options, photo, photos.len(), spec, groups, *first_group, *group_count, slide_index, slide_total),
        SlideContent::Group { group, first_row, row_count } =>
            return group_slide_xml(options, photo, photos.len(), spec, group, *first_row, *row_count, slide_index, slide_total),
        SlideContent::GroupAnnotations { group, first_annotation, annotation_count } =>
            return group_annotations_slide_xml(options, photo, photos.len(), spec, group,
                *first_annotation, *annotation_count, slide_index, slide_total),
        SlideContent::Audit { lines, first_line, line_count } =>
            return audit_slide_xml(options, lines.as_slice(), *first_line, *line_count, slide_index, slide_total),
        SlideContent::Photo => {}
    }
    let mut body = String::from("<p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>");
    let mut next_id = 2u32;
    let mut y = 285_000i64;
    if options.include_title && !options.title.trim().is_empty() {
        body.push_str(&text_shape(next_id, "Titre du rapport", 410_000, y, 11_300_000, 330_000,
            &options.title, "1F4E8C", 24, true, None));
        next_id += 1;
        y += 340_000;
    }
    let mut summary = Vec::new();
    if options.include_folder_name { summary.push(options.folder_name.clone()); }
    if options.include_picture_count { summary.push(format!("{} image(s)", photos.len())); }
    if options.include_generation_date { summary.push(format!("Généré le {}", crate::report::utc_timestamp_for_export())); }
    if !summary.is_empty() {
        body.push_str(&text_shape(next_id, "Contexte rapport", 410_000, y, 11_300_000, 220_000,
            &summary.join("  ·  "), "52657F", 11, false, None));
        next_id += 1;
        y += 240_000;
    }
    if options.include_sequence_numbers || options.include_file_names {
        let mut heading = Vec::new();
        if options.include_sequence_numbers {
            // A folder photo can have continuation slides; use its stable photo ordinal.
            heading.push(format!("Image {} / {}", spec.photo_number, photos.len()));
        }
        if options.include_file_names { heading.push(photo.name.clone()); }
        body.push_str(&text_shape(next_id, "Photo", 410_000, y, 11_300_000, 250_000,
            &heading.join("  ·  "), "273F5E", 15, true, None));
        next_id += 1;
        y += 260_000;
    }
    if options.include_descriptions && !photo.description.trim().is_empty() {
        body.push_str(&text_shape(next_id, "Description", 410_000, y, 11_300_000, 230_000,
            &photo.description, "52657F", 10, false, None));
        next_id += 1;
        y += 230_000;
    }
    if let Some(warning) = photo.warning.as_deref().filter(|warning| !warning.trim().is_empty()) {
        body.push_str(&text_shape(next_id, "Avertissement de source", 410_000, y, 11_300_000, 230_000,
            warning, "B42318", 10, true, Some("FEF3F2")));
        next_id += 1;
        y += 230_000;
    }
    if let Some(notice) = photo.series_notice.as_deref() {
        body.push_str(&text_shape(next_id, "Comparabilité de la série", 410_000, y, 11_300_000, 230_000,
            notice, "8A5A10", 9, false, Some("FFF7E0")));
        next_id += 1;
        y += 230_000;
    }
    if photo.include_timestamp {
        if let Some(timestamp) = photo.timestamp.as_deref() {
            body.push_str(&text_shape(next_id, "Horodatage", 410_000, y, 11_300_000, 200_000,
                timestamp, "52657F", 9, false, None));
            next_id += 1;
            y += 210_000;
        }
    }

    if matches!(&spec.content, SlideContent::Photo) {
        let image_top = (y + 150_000).clamp(950_000, 2_000_000);
        let image_height = (4_280_000 - image_top).max(1_500_000);
        let image_box = fit_image(photo.width, photo.height, 410_000, image_top, 11_300_000, image_height);
        body.push_str(&picture_shape(next_id, image_box, "rId2"));
        next_id += 1;
    }
    body.push_str(&text_shape(next_id, "Pied de page DPER_Measure", 410_000, 6_605_000, 11_300_000, 150_000,
        &format!("DPER_Measure · Coordonnées en pixels natifs · Diapositive {} / {} · U élargie requise pour un verdict", slide_index + 1, slide_total),
        "63738A", 8, false, None));
    body.push_str("</p:spTree>");
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:sld xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:cSld><p:bg><p:bgPr><a:solidFill><a:srgbClr val=\"F4F7FB\"/></a:solidFill><a:effectLst/></p:bgPr></p:bg>{body}</p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>")
}

fn reference_slide_xml(photo: &FolderReportPhoto, page_index: Option<usize>, first_row: usize, row_count: usize, slide_index: usize, slide_total: usize) -> String {
    let Some(reference) = &photo.reference else { return slide_xml_without_reference(); };
    let mut body = String::from("<p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>");
    let mut next_id = 2u32;
    body.push_str(&text_shape(next_id, "Document de référence", 410_000, 285_000, 11_300_000, 330_000,
        &format!("Document de référence · {}", reference.source_name), "1F4E8C", 21, true, None));
    next_id += 1;
    let overall_status = if reference.dimensions.iter().any(|dimension|
        crate::report::is_vision_measurement(&photo.session, dimension.measurement_id)) {
        "Verdict suspendu : mesures vision"
    } else { &reference.overall_status };
    let page_text = if let Some(index) = page_index {
        format!("Page {} du document source · Pièce : {}", index + 1, overall_status)
    } else {
        format!("Comparaison des cotes · Pièce : {}", overall_status)
    };
    body.push_str(&text_shape(next_id, "Page du plan", 410_000, 650_000, 11_300_000, 210_000,
        &page_text, "52657F", 10, false, None));
    next_id += 1;
    if let Some(error) = &reference.source_error {
        body.push_str(&text_shape(next_id, "Source du plan indisponible", 410_000, 850_000, 11_300_000, 260_000,
            &format!("Document source non rendu : {error}"), "B42318", 10, true, Some("FEF3F2")));
        next_id += 1;
    }
    if let Some(page_index) = page_index {
        if let Some(page) = reference.pages.get(page_index) {
            let image_box = fit_image(page.width, page.height, 410_000, 950_000, 11_300_000, 3_280_000);
            body.push_str(&picture_shape(next_id, image_box, "rId2"));
            next_id += 1;
        }
    }
    if row_count > 0 || reference.dimensions.is_empty() {
        let table_y = if page_index.is_some() { 4_430_000 } else { 1_250_000 };
        body.push_str(&reference_table_shapes(reference, &photo.session, first_row, row_count, table_y, &mut next_id));
    }
    let verdict_notice = reference.verdict_notice.as_deref()
        .or(photo.series_notice.as_deref())
        .unwrap_or("Décision par intervalle mesuré ± U; sans U élargie documentée, statut à confirmer.");
    body.push_str(&text_shape(next_id, "Mention comparaison", 410_000, 6_390_000, 11_300_000, 230_000,
        verdict_notice, "8A5A10", 9, false, Some("FFF7E0")));
    next_id += 1;
    body.push_str(&text_shape(next_id, "Pied de page DPER_Measure", 410_000, 6_655_000, 11_300_000, 150_000,
        &format!("DPER_Measure · Diapositive {} / {}", slide_index + 1, slide_total),
        "63738A", 8, false, None));
    body.push_str("</p:spTree>");
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:sld xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:cSld><p:bg><p:bgPr><a:solidFill><a:srgbClr val=\"F4F7FB\"/></a:solidFill><a:effectLst/></p:bgPr></p:bg>{body}</p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>")
}

fn slide_xml_without_reference() -> String {
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:sld xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>{}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>", text_shape(2, "Document absent", 410_000, 285_000, 11_300_000, 360_000, "Document de référence absent", "B42318", 18, true, None))
}

fn reference_table_shapes(reference: &crate::report::FolderReportReference, session: &crate::storage::Session, first_row: usize, row_count: usize, top: i64, next_id: &mut u32) -> String {
    let mut xml = String::new();
    let widths = [3_700_000, 3_350_000, 1_650_000, 2_970_000];
    let headings = ["Cote", "Plage attendue", "Mesure", "Résultat"];
    let mut x = TABLE_X;
    for (column, heading) in headings.iter().enumerate() {
        xml.push_str(&text_shape(*next_id, "En-tête comparaison", x, top, widths[column], 250_000,
            heading, "FFFFFF", 9, true, Some("17385E")));
        *next_id += 1;
        x += widths[column];
    }
    let row_height = 240_000;
    if reference.dimensions.is_empty() {
        xml.push_str(&text_shape(*next_id, "Aucune cote détectée", TABLE_X, top + 260_000, TABLE_W, row_height,
            "Aucune cote ± tolérance détectée; les cotes peuvent être ajoutées dans l'application.",
            "52657F", 9, false, Some("F8FAFC")));
        *next_id += 1;
        return xml;
    }
    let end = (first_row + row_count).min(reference.dimensions.len());
    for (row_index, dimension) in reference.dimensions.iter().enumerate()
        .take(end).skip(first_row)
    {
        let y = top + 260_000 + (row_index - first_row) as i64 * row_height;
        let shade = if (row_index - first_row) % 2 == 0 { "F8FAFC" } else { "FFFFFF" };
        let range = format!("N {:.3} -{:.3}/+{:.3} mm", dimension.nominal_mm,
            dimension.tolerance_minus_mm, dimension.tolerance_plus_mm);
        let measured = dimension.measured_mm.map(|value| dimension.expanded_uncertainty_mm
            .map(|uncertainty| format!("{value:.3} ± {uncertainty:.3} mm"))
            .unwrap_or_else(|| format!("{value:.3} mm · U inconnue")))
            .unwrap_or_else(|| "—".into());
        let status = if crate::report::is_vision_measurement(session, dimension.measurement_id) {
            "Verdict suspendu"
        } else { &dimension.status };
        let status_color = if status == "Dans la tolérance" { "20804A" }
            else if status == "Hors tolérance" { "B42318" } else { "8A5A10" };
        let values = [dimension.label.as_str(), &range, &measured, status];
        let mut x = TABLE_X;
        for column in 0..4 {
            xml.push_str(&text_shape(*next_id, "Résultat comparaison", x, y, widths[column], row_height,
                values[column], if column == 3 { status_color } else { "273F5E" }, 8,
                column == 0 || column == 3, Some(shade)));
            *next_id += 1;
            x += widths[column];
        }
    }
    xml
}

fn table_shapes(photo: &FolderReportPhoto, first_row: usize, row_count: usize, top: i64, next_id: &mut u32) -> String {
    let mut xml = String::new();
    let headings = ["Référence", "Outil", "Résultat", "Étalonnage", "Capture / profil"];
    let mut x = TABLE_X;
    let header_h = 260_000;
    for (column, heading) in headings.iter().enumerate() {
        xml.push_str(&text_shape(*next_id, "En-tête tableau", x, top, TABLE_COLS[column], header_h,
            heading, "FFFFFF", 9, true, Some("17385E")));
        *next_id += 1;
        x += TABLE_COLS[column];
    }
    let row_h = 235_000;
    if photo.session.measurements.is_empty() {
        if row_count > 0 {
            xml.push_str(&text_shape(*next_id, "Aucune mesure", TABLE_X, top + header_h, TABLE_W, row_h,
                "Aucune mesure associée à cette image.", "52657F", 9, false, Some("F8FAFC")));
            *next_id += 1;
        }
        return xml;
    }
    let end = (first_row + row_count).min(photo.session.measurements.len());
    for (row_index, measurement) in photo.session.measurements.iter().enumerate().take(end).skip(first_row) {
        let y = top + header_h + (row_index - first_row) as i64 * row_h;
        let shade = if (row_index - first_row) % 2 == 0 { "F8FAFC" } else { "FFFFFF" };
        let id_reference = format!("M{}", measurement.id);
        let reference = if measurement.label.trim().is_empty() || measurement.label == id_reference {
            id_reference.clone()
        } else { format!("{} ({id_reference})", measurement.label.trim()) };
        let calibration = measurement.mm_per_pixel
            .filter(|value| value.is_finite() && *value > 0.0)
            .map(|value| {
                let profile = measurement.calibration_id.as_ref()
                    .and_then(|id| photo.session.calibrations.iter().find(|profile| &profile.id == id));
                match profile {
                    Some(profile) => {
                        let axes = match (profile.scale_x_mm_per_pixel, profile.scale_y_mm_per_pixel) {
                            (Some(x), Some(y)) => format!("X {x:.8} · Y {y:.8} mm/px"),
                            _ => format!("{value:.8} mm/px"),
                        };
                        let status = match profile.validation_status.as_str() {
                            "verified" => "contrôle validé",
                            "failed" => "contrôle échoué",
                            "unverified" => "à confirmer · U absent",
                            _ => "non vérifié",
                        };
                        let check = if !profile.corner_verification_residuals_mm.is_empty() {
                            format!(" · coins [{}] mm", profile.corner_verification_residuals_mm.iter()
                                .map(|residual| format!("{residual:+.4}"))
                                .collect::<Vec<_>>().join(" / "))
                        } else {
                            profile.verification_residual_mm.map(|residual|
                                format!(" · résidu {residual:+.4} mm")).unwrap_or_default()
                        };
                        let reference_image = std::path::Path::new(&profile.reference_image).file_name()
                            .and_then(|name| name.to_str()).unwrap_or("image non renseignée");
                        format!("{axes} · réf. {} {:.4} mm / {:.2} px · {status}{check} · auteur : {} · {} · {}×{} · {}",
                            profile.reference_type, profile.reference_mm, profile.reference_px, profile.author,
                            reference_image, profile.resolution[0], profile.resolution[1],
                            crate::report::utc_timestamp_at(profile.created_at_unix_secs))
                    }
                    None => format!("{value:.8} mm/px · provenance du profil absente"),
                }
            })
            .unwrap_or_else(|| "Non étalonné · pixels".into());
        let profile = measurement.calibration_id.as_deref().unwrap_or("aucun");
        let method = measurement.method.as_deref().filter(|value| !value.trim().is_empty())
            .map(|value| format!(" · méthode : {value}")).unwrap_or_default();
        let context = format!("{} / {} · origine : {}{}", measurement.capture_id, profile, measurement.origin, method);
        let values = [reference, measurement.kind.label().to_owned(), geometry::result(measurement), calibration, context];
        let mut x = TABLE_X;
        for column in 0..5 {
            xml.push_str(&text_shape(*next_id, "Ligne de mesure", x, y, TABLE_COLS[column], row_h,
                &values[column], if column == 0 { PALETTE[(measurement.id.saturating_sub(1) as usize) % PALETTE.len()] } else { "273F5E" },
                8, column == 0, Some(shade)));
            *next_id += 1;
            x += TABLE_COLS[column];
        }
    }
    xml
}

fn measurement_vectors(measurement: &Measurement, image_box: (i64, i64, i64, i64), source_w: u32, source_h: u32, next_id: &mut u32) -> String {
    let mut xml = String::new();
    let points = measurement.points.iter().map(|point| map_point(*point, image_box, source_w, source_h)).collect::<Vec<_>>();
    let color = PALETTE[(measurement.id.saturating_sub(1) as usize) % PALETTE.len()];
    match measurement.kind {
        Tool::ObjectCount => {}
        Tool::Circle if points.len() >= 2 => {
            // A PPTX shape has no general clipping path. Approximate the circle with
            // clipped vector segments so an out-of-image arc cannot cover report text.
            let center = measurement.points[0];
            let radius = center.dist(measurement.points[1]);
            let sample_count = 256usize;
            let samples = (0..=sample_count).map(|index| {
                let angle = std::f64::consts::TAU * index as f64 / sample_count as f64;
                Point { x: center.x + radius * angle.cos(), y: center.y + radius * angle.sin() }
            }).collect::<Vec<_>>();
            let mut arc = Vec::new();
            for segment in samples.windows(2) {
                let clipped = clip_segment_to_image(segment[0], segment[1], source_w as f64, source_h as f64);
                if let Some((start, end)) = clipped {
                    let start = map_point(start, image_box, source_w, source_h);
                    let end = map_point(end, image_box, source_w, source_h);
                    if arc.last().is_some_and(|last: &(i64, i64)| {
                        (last.0 - start.0).abs() <= 2_000 && (last.1 - start.1).abs() <= 2_000
                    }) {
                        arc.push(end);
                    } else {
                        if arc.len() >= 2 {
                            xml.push_str(&path_shape(*next_id, &arc, false, color));
                            *next_id += 1;
                        }
                        arc = vec![start, end];
                    }
                } else if arc.len() >= 2 {
                    xml.push_str(&path_shape(*next_id, &arc, false, color));
                    *next_id += 1;
                    arc.clear();
                } else {
                    arc.clear();
                }
            }
            if arc.len() >= 2 {
                xml.push_str(&path_shape(*next_id, &arc, false, color));
                *next_id += 1;
            }
            xml.push_str(&path_shape(*next_id, &[points[0], points[1]], false, color));
            *next_id += 1;
        }
        Tool::Rectangle if points.len() >= 2 => {
            let (a, b) = (points[0], points[1]);
            let corners = [(a.0, a.1), (b.0, a.1), (b.0, b.1), (a.0, b.1)];
            xml.push_str(&path_shape(*next_id, &corners, true, color));
            *next_id += 1;
        }
        Tool::Polygon if points.len() >= 3 => {
            xml.push_str(&path_shape(*next_id, &points, true, color));
            *next_id += 1;
        }
        Tool::CircleFit | Tool::Ellipse | Tool::Arc => {
            if let Some(samples) = crate::geometry::sampled_curve(measurement.kind, &measurement.points, 128) {
                let mut path = Vec::new();
                for segment in samples.windows(2) {
                    if let Some((start, end)) = clip_segment_to_image(segment[0], segment[1], source_w as f64, source_h as f64) {
                        let start = map_point(start, image_box, source_w, source_h);
                        let end = map_point(end, image_box, source_w, source_h);
                        if path.last().is_some_and(|last: &(i64, i64)| {
                            (last.0 - start.0).abs() <= 2_000 && (last.1 - start.1).abs() <= 2_000
                        }) {
                            path.push(end);
                        } else {
                            if path.len() >= 2 {
                                xml.push_str(&path_shape(*next_id, &path, false, color));
                                *next_id += 1;
                            }
                            path = vec![start, end];
                        }
                    } else if path.len() >= 2 {
                        xml.push_str(&path_shape(*next_id, &path, false, color));
                        *next_id += 1;
                        path.clear();
                    } else { path.clear(); }
                }
                if path.len() >= 2 {
                    xml.push_str(&path_shape(*next_id, &path, false, color));
                    *next_id += 1;
                }
            }
        }
        Tool::PointLineDistance if points.len() >= 3 => {
            xml.push_str(&path_shape(*next_id, &[points[1], points[2]], false, color));
            *next_id += 1;
            let original = measurement.points[0];
            let first = measurement.points[1];
            let second = measurement.points[2];
            let axis = Point { x: second.x - first.x, y: second.y - first.y };
            let t = ((original.x - first.x) * axis.x + (original.y - first.y) * axis.y)
                / (axis.x * axis.x + axis.y * axis.y);
            if let Some((start, end)) = clip_segment_to_image(original,
                Point { x: first.x + axis.x * t, y: first.y + axis.y * t }, source_w as f64, source_h as f64) {
                let line = [map_point(start, image_box, source_w, source_h), map_point(end, image_box, source_w, source_h)];
                xml.push_str(&path_shape(*next_id, &line, false, color));
                *next_id += 1;
            }
        }
        Tool::ParallelDistance if points.len() >= 4 => {
            for line in [[points[0], points[1]], [points[2], points[3]]] {
                xml.push_str(&path_shape(*next_id, &line, false, color));
                *next_id += 1;
            }
        }
        Tool::Coordinate => {
            if let Some((x, y)) = points.first().copied() {
                xml.push_str(&path_shape(*next_id, &[(x - 18_000, y), (x + 18_000, y)], false, color));
                *next_id += 1;
                xml.push_str(&path_shape(*next_id, &[(x, y - 18_000), (x, y + 18_000)], false, color));
                *next_id += 1;
            }
        }
        _ => {
            xml.push_str(&path_shape(*next_id, &points, false, color));
            *next_id += 1;
        }
    }
    for (x, y) in &points {
        xml.push_str(&ellipse_shape(*next_id, x - 18_000, y - 18_000, 36_000, 36_000, color));
        *next_id += 1;
    }
    if let Some((x, y)) = points.last().copied() {
        let text = format!("M{}", measurement.id);
        let label_w = 510_000;
        let label_h = 180_000;
        let label_x = (x + 36_000).max(image_box.0).min((image_box.0 + image_box.2 - label_w).max(image_box.0));
        let label_y = (y - 120_000).max(image_box.1).min((image_box.1 + image_box.3 - label_h).max(image_box.1));
        xml.push_str(&text_shape(*next_id, "Référence mesure", label_x, label_y, label_w, label_h,
            &text, "FFFFFF", 9, true, Some(color)));
        *next_id += 1;
    }
    xml
}

fn clip_segment_to_image(start: Point, end: Point, width: f64, height: f64) -> Option<(Point, Point)> {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let mut t0: f64 = 0.0;
    let mut t1: f64 = 1.0;
    for (p, q) in [(-dx, start.x), (dx, width - start.x), (-dy, start.y), (dy, height - start.y)] {
        if p == 0.0 {
            if q < 0.0 { return None; }
            continue;
        }
        let ratio = q / p;
        if p < 0.0 {
            if ratio > t1 { return None; }
            t0 = t0.max(ratio);
        } else {
            if ratio < t0 { return None; }
            t1 = t1.min(ratio);
        }
    }
    if t0 > t1 { return None; }
    Some((
        Point { x: start.x + t0 * dx, y: start.y + t0 * dy },
        Point { x: start.x + t1 * dx, y: start.y + t1 * dy },
    ))
}

fn map_point(point: Point, image_box: (i64, i64, i64, i64), source_w: u32, source_h: u32) -> (i64, i64) {
    (image_box.0 + (point.x * image_box.2 as f64 / source_w as f64).round() as i64,
     image_box.1 + (point.y * image_box.3 as f64 / source_h as f64).round() as i64)
}

fn rectangles_overlap(a: (i64, i64, i64, i64), b: (i64, i64, i64, i64)) -> bool {
    a.0 < b.2 && b.0 < a.2 && a.1 < b.3 && b.1 < a.3
}

fn path_shape(id: u32, points: &[(i64, i64)], close: bool, color: &str) -> String {
    if points.len() < 2 { return String::new(); }
    let mut path = format!("<a:moveTo><a:pt x=\"{}\" y=\"{}\"/></a:moveTo>", points[0].0, points[0].1);
    for (x, y) in points.iter().skip(1) { path.push_str(&format!("<a:lnTo><a:pt x=\"{x}\" y=\"{y}\"/></a:lnTo>")); }
    if close { path.push_str("<a:close/>"); }
    format!("<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"Mesure vectorielle\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{SLIDE_W}\" cy=\"{SLIDE_H}\"/></a:xfrm><a:custGeom><a:avLst/><a:gdLst/><a:ahLst/><a:cxnLst/><a:rect l=\"0\" t=\"0\" r=\"{SLIDE_W}\" b=\"{SLIDE_H}\"/><a:pathLst><a:path w=\"{SLIDE_W}\" h=\"{SLIDE_H}\" fill=\"none\" stroke=\"1\">{path}</a:path></a:pathLst></a:custGeom><a:noFill/><a:ln w=\"18000\"><a:solidFill><a:srgbClr val=\"{color}\"/></a:solidFill><a:prstDash val=\"solid\"/></a:ln></p:spPr></p:sp>")
}

fn ellipse_shape(id: u32, x: i64, y: i64, w: i64, h: i64, color: &str) -> String {
    format!("<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"Cercle mesure\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{w}\" cy=\"{h}\"/></a:xfrm><a:prstGeom prst=\"ellipse\"><a:avLst/></a:prstGeom><a:noFill/><a:ln w=\"18000\"><a:solidFill><a:srgbClr val=\"{color}\"/></a:solidFill></a:ln></p:spPr></p:sp>")
}

fn text_shape(id: u32, name: &str, x: i64, y: i64, w: i64, h: i64, text: &str, color: &str, size: u32, bold: bool, fill: Option<&str>) -> String {
    let fill_xml = fill.map(|value| format!("<a:solidFill><a:srgbClr val=\"{value}\"/></a:solidFill>"))
        .unwrap_or_else(|| "<a:noFill/>".into());
    let bold_xml = if bold { " b=\"1\"" } else { "" };
    let paragraphs = text.split('\n').map(|line| format!(
        "<a:p><a:pPr algn=\"l\"/><a:r><a:rPr lang=\"fr-FR\" sz=\"{size}00\"{bold_xml}><a:solidFill><a:srgbClr val=\"{color}\"/></a:solidFill><a:latin typeface=\"Aptos\"/></a:rPr><a:t xml:space=\"preserve\">{}</a:t></a:r><a:endParaRPr lang=\"fr-FR\"/></a:p>", xml_escape(line))).collect::<String>();
    format!("<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"{}\"/><p:cNvSpPr txBox=\"1\"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{w}\" cy=\"{h}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom>{fill_xml}<a:ln><a:noFill/></a:ln></p:spPr><p:txBody><a:bodyPr wrap=\"square\" anchor=\"ctr\" lIns=\"65000\" rIns=\"45000\" tIns=\"12000\" bIns=\"12000\"><a:normAutofit/></a:bodyPr><a:lstStyle/>{paragraphs}</p:txBody></p:sp>", xml_escape(name))
}

fn picture_shape(id: u32, bbox: (i64, i64, i64, i64), rel_id: &str) -> String {
    format!("<p:pic><p:nvPicPr><p:cNvPr id=\"{id}\" name=\"Image source\" descr=\"Image de mesure DPER_Measure\"/><p:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed=\"{rel_id}\"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:ln><a:solidFill><a:srgbClr val=\"D1DCE8\"/></a:solidFill><a:prstDash val=\"solid\"/></a:ln></p:spPr></p:pic>", bbox.0, bbox.1, bbox.2, bbox.3)
}

fn fit_image(width: u32, height: u32, x: i64, y: i64, max_w: i64, max_h: i64) -> (i64, i64, i64, i64) {
    let scale = (max_w as f64 / width as f64).min(max_h as f64 / height as f64);
    let draw_w = (width as f64 * scale).round() as i64;
    let draw_h = (height as f64 * scale).round() as i64;
    (x + (max_w - draw_w) / 2, y + (max_h - draw_h) / 2, draw_w, draw_h)
}

fn content_types(slide_count: usize) -> String {
    let mut slides = String::new();
    for index in 1..=slide_count {
        slides.push_str(&format!("<Override PartName=\"/ppt/slides/slide{index}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>"));
    }
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"png\" ContentType=\"image/png\"/><Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml\"/><Override PartName=\"/ppt/presProps.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presProps+xml\"/><Override PartName=\"/ppt/slideMasters/slideMaster1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml\"/><Override PartName=\"/ppt/slideLayouts/slideLayout1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml\"/><Override PartName=\"/ppt/theme/theme1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.theme+xml\"/>{slides}</Types>")
}

fn root_relationships() -> String {
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"ppt/presentation.xml\"/></Relationships>".into()
}

fn presentation_xml(slide_count: usize) -> String {
    let mut ids = String::new();
    for index in 0..slide_count { ids.push_str(&format!("<p:sldId id=\"{}\" r:id=\"rId{}\"/>", 256 + index, 2 + index)); }
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:presentation xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:sldMasterIdLst><p:sldMasterId id=\"2147483648\" r:id=\"rId1\"/></p:sldMasterIdLst><p:sldIdLst>{ids}</p:sldIdLst><p:sldSz cx=\"{SLIDE_W}\" cy=\"{SLIDE_H}\" type=\"screen16x9\"/><p:notesSz cx=\"6858000\" cy=\"9144000\"/><p:defaultTextStyle><a:defPPr><a:defRPr lang=\"fr-FR\"/></a:defPPr></p:defaultTextStyle></p:presentation>")
}

fn presentation_relationships(slide_count: usize) -> String {
    let mut rels = String::from("<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster\" Target=\"slideMasters/slideMaster1.xml\"/>");
    for index in 0..slide_count { rels.push_str(&format!("<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide\" Target=\"slides/slide{}.xml\"/>", 2 + index, index + 1)); }
    rels.push_str(&format!("<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/presProps\" Target=\"presProps.xml\"/>", slide_count + 2));
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{rels}</Relationships>")
}

fn slide_relationships(spec: &SlideSpec) -> String {
    let image_rel = match &spec.content {
        SlideContent::Reference { page: Some(page), .. } => format!(
            "<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"../media/reference{}_{}.png\"/>",
            spec.photo + 1, page + 1),
        SlideContent::Photo | SlideContent::GroupOverview { .. } | SlideContent::Group { .. } => format!(
            "<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"../media/image{}.png\"/>",
            spec.photo + 1),
        SlideContent::GroupAnnotations { .. } | SlideContent::Reference { page: None, .. }
            | SlideContent::Audit { .. } => String::new(),
    };
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout\" Target=\"../slideLayouts/slideLayout1.xml\"/>{image_rel}</Relationships>")
}

fn slide_master() -> String {
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:sldMaster xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:cSld name=\"DPER_Measure\"><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr></p:spTree></p:cSld><p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/><p:sldLayoutIdLst><p:sldLayoutId id=\"2147483649\" r:id=\"rId1\"/></p:sldLayoutIdLst><p:txStyles><p:titleStyle/><p:bodyStyle/><p:otherStyle/></p:txStyles></p:sldMaster>".into()
}

fn master_relationships() -> String {
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout\" Target=\"../slideLayouts/slideLayout1.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme\" Target=\"../theme/theme1.xml\"/></Relationships>".into()
}

fn slide_layout() -> String {
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:sldLayout xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\" type=\"blank\" preserve=\"1\"><p:cSld name=\"Blank\"><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>".into()
}

fn layout_relationships() -> String {
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster\" Target=\"../slideMasters/slideMaster1.xml\"/></Relationships>".into()
}

fn theme() -> String {
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" name=\"DPER_Measure\"><a:themeElements><a:clrScheme name=\"DPER\"><a:dk1><a:srgbClr val=\"172B45\"/></a:dk1><a:lt1><a:srgbClr val=\"FFFFFF\"/></a:lt1><a:dk2><a:srgbClr val=\"273F5E\"/></a:dk2><a:lt2><a:srgbClr val=\"F4F7FB\"/></a:lt2><a:accent1><a:srgbClr val=\"009FC7\"/></a:accent1><a:accent2><a:srgbClr val=\"E69E00\"/></a:accent2><a:accent3><a:srgbClr val=\"C7334D\"/></a:accent3><a:accent4><a:srgbClr val=\"29915C\"/></a:accent4><a:accent5><a:srgbClr val=\"734FBA\"/></a:accent5><a:accent6><a:srgbClr val=\"2E6BB8\"/></a:accent6><a:hlink><a:srgbClr val=\"0563C1\"/></a:hlink><a:folHlink><a:srgbClr val=\"954F72\"/></a:folHlink></a:clrScheme><a:fontScheme name=\"DPER\"><a:majorFont><a:latin typeface=\"Aptos Display\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:majorFont><a:minorFont><a:latin typeface=\"Aptos\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:minorFont></a:fontScheme><a:fmtScheme name=\"DPER\"><a:fillStyleLst><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln w=\"6350\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill><a:prstDash val=\"solid\"/></a:ln><a:ln w=\"12700\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill><a:prstDash val=\"solid\"/></a:ln><a:ln w=\"19050\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill><a:prstDash val=\"solid\"/></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme></a:themeElements></a:theme>".into()
}

fn xml_escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        let valid = matches!(ch, '\u{9}' | '\u{A}' | '\u{D}')
            || ('\u{20}'..='\u{D7FF}').contains(&ch)
            || ('\u{E000}'..='\u{FFFD}').contains(&ch)
            || ('\u{10000}'..='\u{10FFFF}').contains(&ch);
        if !valid { continue; }
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

struct StoredZip {
    writer: BufWriter<File>,
    central: Vec<u8>,
    entry_count: u16,
    offset: u64,
}

impl StoredZip {
    fn new(file: File) -> Self {
        Self { writer: BufWriter::new(file), central: Vec::new(), entry_count: 0, offset: 0 }
    }

    fn add(&mut self, name: impl Into<String>, data: Vec<u8>) -> Result<(), String> {
        let name = name.into();
        if name.len() > u16::MAX as usize || data.len() > u32::MAX as usize || self.entry_count == u16::MAX {
            return Err("Le paquet PowerPoint dépasse les limites ZIP prises en charge".into());
        }
        let local_length = 30u64 + name.len() as u64 + data.len() as u64;
        let next_offset = self.offset.checked_add(local_length)
            .ok_or_else(|| "Présentation trop volumineuse pour ZIP32".to_owned())?;
        if next_offset > u32::MAX as u64 {
            return Err("Présentation trop volumineuse pour ZIP32".into());
        }

        let name_bytes = name.as_bytes();
        let checksum = crc32(&data);
        let local_offset = self.offset as u32;
        let mut local = Vec::with_capacity(30 + name_bytes.len());
        put_u32(&mut local, 0x0403_4b50);
        put_u16(&mut local, 20); put_u16(&mut local, 0x0800); put_u16(&mut local, 0);
        put_u16(&mut local, 0); put_u16(&mut local, 0x5ca1);
        put_u32(&mut local, checksum); put_u32(&mut local, data.len() as u32); put_u32(&mut local, data.len() as u32);
        put_u16(&mut local, name_bytes.len() as u16); put_u16(&mut local, 0);
        local.extend_from_slice(name_bytes);
        self.writer.write_all(&local).and_then(|_| self.writer.write_all(&data))
            .map_err(|error| format!("Écriture du PowerPoint impossible : {error}"))?;

        let mut central_entry = Vec::with_capacity(46 + name_bytes.len());
        put_u32(&mut central_entry, 0x0201_4b50);
        put_u16(&mut central_entry, 20); put_u16(&mut central_entry, 20); put_u16(&mut central_entry, 0x0800);
        put_u16(&mut central_entry, 0); put_u16(&mut central_entry, 0); put_u16(&mut central_entry, 0x5ca1);
        put_u32(&mut central_entry, checksum); put_u32(&mut central_entry, data.len() as u32); put_u32(&mut central_entry, data.len() as u32);
        put_u16(&mut central_entry, name_bytes.len() as u16); put_u16(&mut central_entry, 0); put_u16(&mut central_entry, 0);
        put_u16(&mut central_entry, 0); put_u16(&mut central_entry, 0); put_u32(&mut central_entry, 0); put_u32(&mut central_entry, local_offset);
        central_entry.extend_from_slice(name_bytes);
        self.central.extend_from_slice(&central_entry);
        self.offset = next_offset;
        self.entry_count += 1;
        Ok(())
    }

    fn finish(mut self) -> Result<(), String> {
        if self.central.len() > u32::MAX as usize {
            return Err("Présentation trop volumineuse pour ZIP32".into());
        }
        let total_size = self.offset.checked_add(self.central.len() as u64)
            .and_then(|size| size.checked_add(22))
            .ok_or_else(|| "Présentation trop volumineuse pour ZIP32".to_owned())?;
        if total_size > u32::MAX as u64 {
            return Err("Présentation trop volumineuse pour ZIP32".into());
        }
        let central_offset = self.offset as u32;
        let central_size = self.central.len() as u32;
        let mut footer = Vec::with_capacity(22);
        put_u32(&mut footer, 0x0605_4b50); put_u16(&mut footer, 0); put_u16(&mut footer, 0);
        put_u16(&mut footer, self.entry_count); put_u16(&mut footer, self.entry_count);
        put_u32(&mut footer, central_size); put_u32(&mut footer, central_offset); put_u16(&mut footer, 0);
        self.writer.write_all(&self.central).and_then(|_| self.writer.write_all(&footer))
            .and_then(|_| self.writer.flush())
            .map_err(|error| format!("Finalisation du PowerPoint impossible : {error}"))
    }
}

fn put_u16(out: &mut Vec<u8>, value: u16) { out.extend_from_slice(&value.to_le_bytes()); }
fn put_u32(out: &mut Vec<u8>, value: u32) { out.extend_from_slice(&value.to_le_bytes()); }

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= *byte as u32;
        for _ in 0..8 { crc = if crc & 1 == 1 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 }; }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::test_support::{dense_options, dense_photo, temporary_path, RemoveOnDrop};

    fn read_u16(bytes: &[u8], offset: usize) -> u16 {
        u16::from_le_bytes(bytes[offset..offset + 2].try_into().expect("u16 ZIP"))
    }

    fn read_u32(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("u32 ZIP"))
    }

    fn read_stored_entries(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
        let mut entries = Vec::new();
        let mut offset = 0usize;
        while offset + 4 <= bytes.len() && read_u32(bytes, offset) == 0x0403_4b50 {
            assert!(offset + 30 <= bytes.len(), "en-tête ZIP local tronqué");
            assert_eq!(read_u16(bytes, offset + 8), 0, "entrée ZIP non stockée");
            let expected_crc = read_u32(bytes, offset + 14);
            let compressed_size = read_u32(bytes, offset + 18) as usize;
            let uncompressed_size = read_u32(bytes, offset + 22) as usize;
            assert_eq!(compressed_size, uncompressed_size);
            let name_len = read_u16(bytes, offset + 26) as usize;
            let extra_len = read_u16(bytes, offset + 28) as usize;
            let name_start = offset + 30;
            let data_start = name_start + name_len + extra_len;
            let data_end = data_start.checked_add(compressed_size).expect("taille ZIP");
            assert!(data_end <= bytes.len(), "données ZIP tronquées");
            let name = String::from_utf8(bytes[name_start..name_start + name_len].to_vec())
                .expect("nom d’entrée ZIP UTF-8");
            let data = bytes[data_start..data_end].to_vec();
            assert_eq!(crc32(&data), expected_crc, "CRC invalide pour {name}");
            entries.push((name, data));
            offset = data_end;
        }

        assert!(offset + 4 <= bytes.len());
        assert_eq!(read_u32(bytes, offset), 0x0201_4b50, "répertoire central ZIP absent");
        assert!(bytes.len() >= 22);
        let footer = bytes.len() - 22;
        assert_eq!(read_u32(bytes, footer), 0x0605_4b50, "fin ZIP absente");
        assert_eq!(read_u16(bytes, footer + 10) as usize, entries.len());
        assert_eq!(read_u32(bytes, footer + 16) as usize, offset);
        entries
    }

    #[test]
    fn dense_pptx_contains_valid_zip_slides_groups_tables_and_statuses() {
        let path = temporary_path("pptx");
        let _cleanup = RemoveOnDrop(path.clone());
        let photo = dense_photo();
        let options = dense_options();

        assert_eq!(write_folder_pptx(&path, &options, &[photo])
            .unwrap_or_else(|error| panic!("PPTX dense ({}) : {error}", path.display())), None);
        if std::env::var_os("DPER_MEASURE_KEEP_TEST_OUTPUTS").is_some() {
            println!("PPTX_FIXTURE_PATH={}", path.display());
        }
        let bytes = fs::read(&path).expect("lecture du PPTX généré");
        let entries = read_stored_entries(&bytes);
        let slide_entries = entries.iter()
            .filter(|(name, _)| name.starts_with("ppt/slides/slide") && name.ends_with(".xml"))
            .collect::<Vec<_>>();
        assert!(slide_entries.len() >= 20, "les groupes doivent disposer de diapositives dédiées");

        let slide_xml = slide_entries.iter()
            .map(|(_, data)| String::from_utf8(data.clone()).expect("XML de diapositive UTF-8"))
            .collect::<Vec<_>>()
            .join("\n");
        for expected in ["Groupe 1", "Groupe 20", "M100", "Mesure / cote", "Dans la tolérance", "Non associée"] {
            assert!(slide_xml.contains(expected), "texte absent des diapositives : {expected}");
        }
        assert!(entries.iter().any(|(name, _)| name == "[Content_Types].xml"));
        assert!(entries.iter().any(|(name, _)| name == "ppt/presentation.xml"));
    }

    #[test]
    fn pptx_suspends_vision_and_includes_notes_when_audit_is_disabled() {
        let options = dense_options();
        assert!(!options.include_audit_page);
        let path = temporary_path("pptx");
        let _cleanup = RemoveOnDrop(path.clone());
        write_folder_pptx(&path, &options, &[crate::report::test_support::historical_vision_photo()]).unwrap();
        let bytes = fs::read(path).unwrap();
        let xml = read_stored_entries(&bytes).into_iter()
            .filter(|(name, _)| name.starts_with("ppt/slides/slide") && name.ends_with(".xml"))
            .map(|(_, data)| String::from_utf8(data).unwrap()).collect::<Vec<_>>().join("\n");
        for marker in ["Mesures vision", "Historique non réévalué", "historique-vision", "Verdict suspendu"] {
            assert!(xml.contains(marker), "Notice PPTX absente : {marker}");
        }
        assert!(!xml.contains(">Dans la tolérance<"));
    }
}
