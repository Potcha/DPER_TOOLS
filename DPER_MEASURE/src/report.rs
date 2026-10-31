//! Génération d'un rapport PDF autonome avec image source et annotations vectorielles.
use crate::{geometry::{self, Measurement, Point, Tool}, reference::RenderedPage, storage::Session};
use image::ImageEncoder;
use std::{fs, path::Path, time::{SystemTime, UNIX_EPOCH}};

const PAGE_WIDTH: f64 = 595.0;
const PAGE_HEIGHT: f64 = 842.0;
const MARGIN: f64 = 36.0;
const TABLE_WIDTH: f64 = PAGE_WIDTH - MARGIN * 2.0;
const COLUMN_WIDTHS: [f64; 5] = [105.0, 90.0, 115.0, 95.0, 118.0];

#[derive(Clone, Copy)]
struct Rgb(f64, f64, f64);

const PALETTE: [Rgb; 8] = [
    Rgb(0.00, 0.63, 0.78), Rgb(0.90, 0.62, 0.00),
    Rgb(0.78, 0.20, 0.30), Rgb(0.16, 0.57, 0.34),
    Rgb(0.45, 0.31, 0.73), Rgb(0.85, 0.36, 0.12),
    Rgb(0.18, 0.42, 0.72), Rgb(0.65, 0.31, 0.53),
];

pub struct FolderReportOptions {
    pub title: String,
    pub include_title: bool,
    pub folder_name: String,
    pub include_folder_name: bool,
    pub include_picture_count: bool,
    pub include_generation_date: bool,
    pub include_sequence_numbers: bool,
    pub include_file_names: bool,
    pub include_descriptions: bool,
    pub include_measurements: bool,
    pub include_audit_page: bool,
    pub operator_name: String,
    pub part_reference: String,
    pub drawing_revision: String,
    pub lot_number: String,
    pub audit_notes: String,
    pub approval_status: String,
    pub approved_by: String,
}

pub struct FolderReportPhoto {
    pub name: String,
    pub description: String,
    pub warning: Option<String>,
    pub timestamp: Option<String>,
    pub include_timestamp: bool,
    pub session: Session,
    pub image_rgb: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub reference: Option<FolderReportReference>,
    pub series_notice: Option<String>,
}

pub struct FolderReportReference {
    pub source_name: String,
    pub source_error: Option<String>,
    pub verdict_notice: Option<String>,
    pub overall_status: String,
    pub pages: Vec<RenderedPage>,
    pub dimensions: Vec<FolderReportReferenceDimension>,
}

pub struct FolderReportReferenceDimension {
    pub id: String,
    pub measurement_id: Option<u64>,
    pub label: String,
    pub source_text: String,
    pub ocr_page: Option<u32>,
    pub ocr_bbox: Option<crate::reference::ReferenceOcrBox>,
    pub ocr_confidence: Option<f64>,
    pub nominal_mm: f64,
    pub tolerance_minus_mm: f64,
    pub tolerance_plus_mm: f64,
    pub measured_mm: Option<f64>,
    pub expanded_uncertainty_mm: Option<f64>,
    pub lower_limit_mm: f64,
    pub upper_limit_mm: f64,
    pub status: String,
}

pub(crate) const VISION_EXPORT_NOTICE: &str = "Mesures vision : preuves à réévaluer, qualification/U non établies ; conformité suspendue.";

pub(crate) fn is_vision_measurement(session: &Session, id: Option<u64>) -> bool {
    id.is_some_and(|id| session.measurements.iter()
        .any(|measurement| measurement.id == id && measurement.origin.eq_ignore_ascii_case("vision")))
}

/// Exported states are historical snapshots, never fresh pixel validation.
pub(crate) fn inspection_export_notes(session: &Session) -> Vec<String> {
    if session.automated_inspection.is_none()
        && !session.measurements.iter().any(|measurement| measurement.origin.eq_ignore_ascii_case("vision"))
    { return Vec::new(); }
    let mut lines = vec![VISION_EXPORT_NOTICE.to_owned()];
    if let Some(workspace) = &session.automated_inspection {
        for record in &workspace.records {
            let object = record.measurement_id.map(|id| format!("M{id}")).unwrap_or_else(|| "aperçu".into());
            lines.push(format!(
                "Historique non réévalué : {} / {} / {} ; géométrie sauvegardée {} ; conformité suspendue ; {}",
                record.group, record.feature_id, object, record.outcome.geometry_state.label(),
                record.outcome.blocking_reasons.iter().map(|reason| crate::dper_metrology::reason_label(reason)).collect::<Vec<_>>().join(" ; ")));
        }
    }
    lines
}

struct ReportImage {
    png_idat: Vec<u8>,
    width: u32,
    height: u32,
}

struct FolderPage {
    content: Vec<u8>,
    image_index: Option<usize>,
}

/// Crée un rapport dossier : une page annotée par image et des pages de suite
/// si le tableau d'une image ne tient pas sous sa photo.
pub fn write_folder_pdf(
    path: &Path,
    options: &FolderReportOptions,
    photos: &[FolderReportPhoto],
) -> Result<(), String> {
    if photos.is_empty() {
        return Err("Sélectionnez au moins une image pour le rapport".into());
    }

    let mut images = Vec::with_capacity(photos.len());
    for photo in photos {
        let expected_len = (photo.width as usize)
            .checked_mul(photo.height as usize)
            .and_then(|pixels| pixels.checked_mul(3))
            .ok_or_else(|| format!("Dimensions trop grandes pour {}", photo.name))?;
        if photo.width == 0 || photo.height == 0 || photo.image_rgb.len() != expected_len {
            return Err(format!("Pixels de l'image source invalides : {}", photo.name));
        }
        images.push(ReportImage {
            png_idat: encode_png_idat(&photo.image_rgb, photo.width, photo.height)?,
            width: photo.width,
            height: photo.height,
        });
    }
    let mut reference_indices = vec![Vec::new(); photos.len()];
    for (photo_index, photo) in photos.iter().enumerate() {
        if let Some(reference) = &photo.reference {
            for page in &reference.pages {
                let expected = (page.width as usize).checked_mul(page.height as usize)
                    .and_then(|pixels| pixels.checked_mul(3))
                    .ok_or_else(|| format!("Dimensions de page de référence trop grandes : {}", reference.source_name))?;
                if page.width == 0 || page.height == 0 || page.rgb.len() != expected {
                    return Err(format!("Pixels invalides dans le document de référence : {}", reference.source_name));
                }
                reference_indices[photo_index].push(images.len());
                images.push(ReportImage {
                    png_idat: encode_png_idat(&page.rgb, page.width, page.height)?,
                    width: page.width,
                    height: page.height,
                });
            }
        }
    }

    let mut pages = Vec::new();
    render_audit_pages(&mut pages, options, photos);
    render_inspection_notice_pages(&mut pages, photos);
    let mut next_group_number = 1usize;
    for (index, photo) in photos.iter().enumerate() {
        render_folder_photo(&mut pages, options, photo, index, photos.len(), &mut next_group_number);
        if let Some(reference) = &photo.reference {
            render_folder_reference(
                &mut pages,
                reference,
                &photo.session,
                &reference_indices[index],
                options.include_measurements,
            );
        }
    }
    let pdf = assemble_folder_pdf(&pages, &images);
    fs::write(path, pdf).map_err(|error| format!("Écriture du rapport PDF impossible : {error}"))
}

fn render_inspection_notice_pages(pages: &mut Vec<FolderPage>, photos: &[FolderReportPhoto]) {
    for photo in photos {
        let lines = inspection_export_notes(&photo.session).into_iter()
            .flat_map(|line| wrap_text(&pdf_compatible_text(&line), 105)).collect::<Vec<_>>();
        for chunk in lines.chunks(55) {
            let mut page = Vec::new();
            draw_text(&mut page, MARGIN, 807.0, 14.0, Rgb(0.10, 0.19, 0.31),
                &shorten(&format!("Inspection vision · {}", photo.name), 95), true);
            for (index, line) in chunk.iter().enumerate() {
                draw_text(&mut page, MARGIN, 783.0 - index as f64 * 12.0, 8.0,
                    Rgb(0.55, 0.29, 0.06), line, false);
            }
            draw_footer(&mut page, pages.len() + 1);
            pages.push(FolderPage { content: page, image_index: None });
        }
    }
}

fn render_audit_pages(
    pages: &mut Vec<FolderPage>,
    options: &FolderReportOptions,
    photos: &[FolderReportPhoto],
) {
    if !options.include_audit_page { return; }
    let mut page = Vec::new();
    let mut page_number = pages.len() + 1;
    let mut y = 807.0;
    let draw_line = |page: &mut Vec<u8>, y: f64, label: &str, value: &str| {
        draw_text(page, MARGIN, y, 8.5, Rgb(0.10, 0.19, 0.31), label, true);
        draw_text(page, MARGIN + 128.0, y, 8.5, Rgb(0.18, 0.22, 0.28),
            &shorten(&pdf_compatible_text(default_dash(value)), 85), false);
    };
    draw_text(&mut page, MARGIN, y, 18.0, Rgb(0.10, 0.19, 0.31), "FICHE DE CONTRÔLE ET D’AUDIT", true);
    y -= 27.0;
    draw_line(&mut page, y, "Opérateur", &options.operator_name); y -= 14.0;
    draw_line(&mut page, y, "Référence pièce", &options.part_reference); y -= 14.0;
    draw_line(&mut page, y, "Révision du plan", &options.drawing_revision); y -= 14.0;
    draw_line(&mut page, y, "Lot", &options.lot_number); y -= 14.0;
    draw_line(&mut page, y, "Décision", &options.approval_status); y -= 14.0;
    draw_line(&mut page, y, "Validé par", &options.approved_by); y -= 14.0;
    draw_line(&mut page, y, "Généré le", &utc_timestamp()); y -= 14.0;
    draw_line(&mut page, y, "Images sélectionnées", &photos.len().to_string()); y -= 20.0;

    if !options.audit_notes.trim().is_empty() {
        draw_text(&mut page, MARGIN, y, 9.0, Rgb(0.10, 0.19, 0.31), "Notes d’inspection", true);
        y -= 14.0;
        for line in wrap_text(&pdf_compatible_text(options.audit_notes.trim()), 105) {
            if y < 60.0 {
                draw_footer(&mut page, page_number);
                pages.push(FolderPage { content: page, image_index: None });
                page = Vec::new(); page_number = pages.len() + 1; y = 807.0;
                draw_text(&mut page, MARGIN, y, 14.0, Rgb(0.10, 0.19, 0.31), "Fiche d’audit · suite", true);
                y -= 23.0;
            }
            draw_text(&mut page, MARGIN + 8.0, y, 8.2, Rgb(0.18, 0.22, 0.28), &line, false);
            y -= 11.0;
        }
        y -= 7.0;
    }

    draw_text(&mut page, MARGIN, y, 9.0, Rgb(0.10, 0.19, 0.31), "Acquisitions, appareils et étalonnages", true);
    y -= 15.0;
    for photo in photos {
        if y < 70.0 {
            draw_footer(&mut page, page_number);
            pages.push(FolderPage { content: page, image_index: None });
            page = Vec::new(); page_number = pages.len() + 1; y = 807.0;
            draw_text(&mut page, MARGIN, y, 14.0, Rgb(0.10, 0.19, 0.31), "Acquisitions · suite", true);
            y -= 23.0;
        }
        let calibration = photo.session.calibration.as_ref().map(|profile| format!(
            "{} · {} · X {:.8} / Y {:.8} mm/px · U {}",
            profile.id, profile.validation_status,
            profile.scale_x_mm_per_pixel.unwrap_or(profile.mm_per_pixel),
            profile.scale_y_mm_per_pixel.unwrap_or(profile.mm_per_pixel),
            profile.expanded_uncertainty_mm.map(|value| format!("{value:.8} mm")).unwrap_or_else(|| "inconnue".into()),
        )).unwrap_or_else(|| "Non étalonné".into());
        draw_text(&mut page, MARGIN, y, 8.0, Rgb(0.10, 0.19, 0.31),
            &shorten(&pdf_compatible_text(&format!("{} · {} × {} px", photo.name, photo.width, photo.height)), 110), true);
        y -= 11.0;
        draw_text(&mut page, MARGIN + 8.0, y, 7.8, Rgb(0.18, 0.22, 0.28),
            &shorten(&pdf_compatible_text(&format!("Appareil : {} [{}] · Optique : {}", photo.session.device, photo.session.device_id, photo.session.optical_setup)), 118), false);
        y -= 10.0;
        draw_text(&mut page, MARGIN + 8.0, y, 7.8, Rgb(0.18, 0.22, 0.28),
            &shorten(&pdf_compatible_text(&format!("Calibration : {calibration}")), 118), false);
        y -= 12.0;
    }

    let mut corrections = photos.iter().flat_map(|photo| photo.session.measurement_audit_history.iter()
        .map(move |event| (photo.name.as_str(), event))).collect::<Vec<_>>();
    corrections.sort_by_key(|(_, event)| event.timestamp_unix_secs);
    if !corrections.is_empty() {
        if y < 90.0 {
            draw_footer(&mut page, page_number);
            pages.push(FolderPage { content: page, image_index: None });
            page = Vec::new(); page_number = pages.len() + 1; y = 807.0;
        }
        draw_text(&mut page, MARGIN, y, 9.0, Rgb(0.10, 0.19, 0.31), "Historique des corrections", true);
        y -= 15.0;
        for (photo_name, event) in corrections {
            if y < 52.0 {
                draw_footer(&mut page, page_number);
                pages.push(FolderPage { content: page, image_index: None });
                page = Vec::new(); page_number = pages.len() + 1; y = 807.0;
                draw_text(&mut page, MARGIN, y, 14.0, Rgb(0.10, 0.19, 0.31), "Historique des corrections · suite", true);
                y -= 23.0;
            }
            let ids = event.measurement_ids.iter().map(|id| format!("M{id}")).collect::<Vec<_>>().join(", ");
            let before = event.before.iter().map(|value| format!("M{} {}={}", value.id, value.label, value.result))
                .collect::<Vec<_>>().join("; ");
            let after = event.after.iter().map(|value| format!("M{} {}={}", value.id, value.label, value.result))
                .collect::<Vec<_>>().join("; ");
            let summary = format!("{} · {} · {} · avant [{}] · après [{}]",
                utc_timestamp_at(event.timestamp_unix_secs), photo_name, event.action, before, after);
            let lines = wrap_text(&pdf_compatible_text(&summary), 115);
            for (line_index, line) in lines.iter().enumerate() {
                if y < 52.0 {
                    draw_footer(&mut page, page_number);
                    pages.push(FolderPage { content: page, image_index: None });
                    page = Vec::new(); page_number = pages.len() + 1; y = 807.0;
                    draw_text(&mut page, MARGIN, y, 14.0, Rgb(0.10, 0.19, 0.31), "Historique des corrections · suite", true);
                    y -= 23.0;
                }
                let prefix = if line_index == 0 { format!("{ids} · ") } else { String::new() };
                draw_text(&mut page, MARGIN + 5.0, y, 7.4, Rgb(0.18, 0.22, 0.28),
                    &shorten(&format!("{prefix}{line}"), 120), false);
                y -= 10.0;
            }
        }
    }
    if y < 65.0 {
        draw_footer(&mut page, page_number);
        pages.push(FolderPage { content: page, image_index: None });
        page = Vec::new(); page_number = pages.len() + 1; y = 807.0;
    }
    y -= 8.0;
    draw_text(&mut page, MARGIN, y, 8.5, Rgb(0.10, 0.19, 0.31),
        &format!("Validateur : {} · signature manuscrite à apposer ci-dessous",
            pdf_compatible_text(default_dash(&options.approved_by))), true);
    y -= 30.0;
    push_ascii(&mut page, "0.45 0.49 0.54 RG 0.7 w\n");
    self::draw_line(&mut page, (MARGIN, y), (MARGIN + 230.0, y));
    draw_text(&mut page, MARGIN, y - 12.0, 7.5, Rgb(0.35, 0.40, 0.47), "Date et signature", false);
    draw_footer(&mut page, page_number);
    pages.push(FolderPage { content: page, image_index: None });
}

fn render_annotation_pages(
    pages: &mut Vec<FolderPage>,
    photo: &FolderReportPhoto,
    image_index: usize,
    group_number: usize,
    group: &ReportGroup,
) {
    if group.annotation_indices.is_empty() { return; }
    let mut page = Vec::new();
    draw_text(&mut page, MARGIN, 807.0, 14.0, Rgb(0.10, 0.19, 0.31),
        &shorten(&format!("Commentaires · Groupe {group_number} · {}", photo.name), 100), true);
    draw_text(&mut page, MARGIN, 789.0, 8.0, Rgb(0.35, 0.40, 0.47),
        &shorten(&format!("{} · notes liées aux cibles du groupe", group.description), 110), false);
    let image_scale = (TABLE_WIDTH / photo.width as f64).min(300.0 / photo.height as f64);
    let draw_width = photo.width as f64 * image_scale;
    let draw_height = photo.height as f64 * image_scale;
    let image_x = (PAGE_WIDTH - draw_width) * 0.5;
    let image_top = 765.0;
    let image_y = image_top - draw_height;
    push_ascii(&mut page, &format!(
        "q\n{draw_width:.3} 0 0 {draw_height:.3} {image_x:.3} {image_y:.3} cm\n/Im0 Do\nQ\n"));
    push_ascii(&mut page, &format!(
        "0.65 0.69 0.74 RG 0.7 w {image_x:.3} {image_y:.3} {draw_width:.3} {draw_height:.3} re S\n"));
    draw_annotation_target_markers(
        &mut page, photo, group, group_number, image_x, image_y, draw_width, draw_height,
    );
    let mut page_image = Some(image_index);
    let mut page_number = pages.len() + 1;
    let mut cursor_y = image_y - 17.0;
    for (index, annotation_index) in group.annotation_indices.iter().enumerate() {
        let Some(annotation) = photo.session.annotations.get(*annotation_index) else { continue; };
        let target = annotation_target_description(annotation, &photo.session.measurements);
        let lines = wrap_text(&pdf_compatible_text(&annotation.text), 105);
        let row_lines = lines.len().max(1);
        let row_height = 19.0 + row_lines as f64 * 9.0;
        if cursor_y - row_height < 42.0 {
            draw_footer(&mut page, page_number);
            pages.push(FolderPage { content: page, image_index: page_image.take() });
            page = Vec::new();
            page_number = pages.len() + 1;
            draw_text(&mut page, MARGIN, 807.0, 14.0, Rgb(0.10, 0.19, 0.31),
                &shorten(&format!("Commentaires · Groupe {group_number} · suite · {}", photo.name), 100), true);
            cursor_y = 780.0;
        }
        let bottom = cursor_y - row_height;
        let shade = if index % 2 == 0 { "0.96 0.97 0.98" } else { "1 1 1" };
        push_ascii(&mut page, &format!(
            "{shade} rg {MARGIN:.2} {bottom:.2} {TABLE_WIDTH:.2} {row_height:.2} re f\n"));
        push_ascii(&mut page, &format!(
            "0.78 0.81 0.84 RG 0.35 w {MARGIN:.2} {bottom:.2} {TABLE_WIDTH:.2} {row_height:.2} re S\n"));
        let annotation_id = annotation_index + 1;
        draw_text(&mut page, MARGIN + 6.0, cursor_y - 11.0, 7.5,
            group_color(group_number), &shorten(&format!("A{annotation_id} · {target}"), 115), true);
        for (line_index, line) in lines.iter().enumerate() {
            draw_text(&mut page, MARGIN + 6.0, cursor_y - 22.0 - line_index as f64 * 9.0,
                7.2, Rgb(0.12, 0.16, 0.21), line, false);
        }
        cursor_y = bottom - 3.0;
    }
    draw_footer(&mut page, page_number);
    pages.push(FolderPage { content: page, image_index: page_image });
}

fn annotation_target_description(annotation: &crate::geometry::Annotation, measurements: &[Measurement]) -> String {
    match &annotation.target {
        crate::geometry::AnnotationTarget::Measurement { measurement_id } => measurements.iter()
            .find(|measurement| measurement.id == *measurement_id)
            .map(|measurement| format!("Mesure M{} · {}", measurement.id, measurement.label))
            .unwrap_or_else(|| format!("Mesure M{measurement_id}")),
        crate::geometry::AnnotationTarget::Point { point } => {
            format!("Point ({:.1}; {:.1}) px", point.x, point.y)
        }
        crate::geometry::AnnotationTarget::Zone { first, second } => format!(
            "Zone ({:.1}; {:.1})–({:.1}; {:.1}) px", first.x, first.y, second.x, second.y,
        ),
    }
}

fn draw_annotation_target_markers(
    page: &mut Vec<u8>,
    photo: &FolderReportPhoto,
    group: &ReportGroup,
    group_number: usize,
    image_x: f64,
    image_y: f64,
    draw_width: f64,
    draw_height: f64,
) {
    let scale_x = draw_width / photo.width as f64;
    let scale_y = draw_height / photo.height as f64;
    let occupied = photo.session.measurements.iter()
        .map(|measurement| measurement_report_bounds(
            measurement, image_x, image_y, scale_x, scale_y, photo.height,
        )).collect::<Vec<_>>();
    let mut occupied_badges = Vec::new();
    for annotation_index in &group.annotation_indices {
        let Some(annotation) = photo.session.annotations.get(*annotation_index) else { continue; };
        let anchor = match &annotation.target {
            crate::geometry::AnnotationTarget::Measurement { measurement_id } => photo.session.measurements
                .iter().find(|measurement| measurement.id == *measurement_id)
                .map(measurement_center),
            crate::geometry::AnnotationTarget::Point { point } => Some((point.x, point.y)),
            crate::geometry::AnnotationTarget::Zone { first, second } =>
                Some(((first.x + second.x) * 0.5, (first.y + second.y) * 0.5)),
        };
        let Some((source_x, source_y)) = anchor else { continue; };
        let target = map_point(
            Point { x: source_x, y: source_y }, image_x, image_y,
            scale_x, scale_y, photo.height,
        );
        let color = group_color(group_number);
        push_ascii(page, &format!("{} {} {} RG 0.8 w\n", color.0, color.1, color.2));
        draw_circle(page, target.0, target.1, 3.4, false);
        push_ascii(page, &format!(
            "{:.3} {:.3} m {:.3} {:.3} l S {:.3} {:.3} m {:.3} {:.3} l S\n",
            target.0 - 2.0, target.1, target.0 + 2.0, target.1,
            target.0, target.1 - 2.0, target.0, target.1 + 2.0,
        ));

        let label = format!("A{}", annotation_index + 1);
        let badge_width = (label.len() as f64 * 4.2 + 6.0).max(17.0);
        let badge_height = 11.0;
        let candidates = [
            (target.0 + 5.0, target.1 + 4.0),
            (target.0 - badge_width - 5.0, target.1 + 4.0),
            (target.0 + 5.0, target.1 - badge_height - 4.0),
            (target.0 - badge_width - 5.0, target.1 - badge_height - 4.0),
            (target.0 + 6.0, target.1 - badge_height * 0.5),
            (target.0 - badge_width - 6.0, target.1 - badge_height * 0.5),
        ];
        let safe_position = candidates.into_iter().find(|(x, y)| {
            let candidate = (*x, *y, *x + badge_width, *y + badge_height);
            candidate.0 >= image_x + 1.0 && candidate.2 <= image_x + draw_width - 1.0
                && candidate.1 >= image_y + 1.0 && candidate.3 <= image_y + draw_height - 1.0
                && occupied.iter().all(|bounds| !rects_overlap(candidate, *bounds))
                && occupied_badges.iter().all(|bounds| !rects_overlap(candidate, *bounds))
        });
        if let Some((x, y)) = safe_position {
            let bounds = (x, y, x + badge_width, y + badge_height);
            occupied_badges.push(bounds);
            push_ascii(page, &format!(
                "{} {} {} rg {x:.3} {y:.3} {badge_width:.3} {badge_height:.3} re f\n",
                color.0, color.1, color.2,
            ));
            draw_text(page, x + 3.0, y + 2.6, 6.8, Rgb(1.0, 1.0, 1.0), &label, true);
        }
    }
}

fn render_folder_photo(
    pages: &mut Vec<FolderPage>,
    options: &FolderReportOptions,
    photo: &FolderReportPhoto,
    image_index: usize,
    selected_count: usize,
    next_group_number: &mut usize,
) {
    if options.include_measurements {
        let groups = build_report_groups(photo);
        let numbered_groups = groups.into_iter().enumerate()
            .map(|(offset, group)| (*next_group_number + offset, group))
            .collect::<Vec<_>>();
        render_group_overview(
            pages, options, photo, image_index, selected_count, &numbered_groups,
        );
        for (group_number, group) in &numbered_groups {
            render_folder_group_photo(
                pages,
                options,
                photo,
                image_index,
                selected_count,
                *group_number,
                group,
                photo.reference.as_ref(),
            );
            render_annotation_pages(pages, photo, image_index, *group_number, group);
        }
        *next_group_number += numbered_groups.len();
        return;
    }

    render_folder_photo_overview(pages, options, photo, image_index, selected_count);
}

fn render_folder_photo_overview(
    pages: &mut Vec<FolderPage>,
    options: &FolderReportOptions,
    photo: &FolderReportPhoto,
    image_index: usize,
    selected_count: usize,
) {
    let mut page = Vec::new();
    let mut header_y = 807.0;
    if options.include_title && !options.title.trim().is_empty() {
        draw_text(&mut page, MARGIN, header_y, 16.0, Rgb(0.10, 0.19, 0.31),
            &shorten(options.title.trim(), 90), true);
        header_y -= 19.0;
    }
    if options.include_folder_name {
        draw_text(&mut page, MARGIN, header_y, 8.5, Rgb(0.17, 0.22, 0.29),
            &format!("Dossier : {}", options.folder_name), false);
        header_y -= 14.0;
    }
    let mut report_details = Vec::new();
    if options.include_picture_count {
        report_details.push(format!("{} image(s)", selected_count));
    }
    if options.include_generation_date {
        report_details.push(format!("Généré le {}", utc_timestamp()));
    }
    if !report_details.is_empty() {
        draw_text(&mut page, MARGIN, header_y, 8.5, Rgb(0.17, 0.22, 0.29),
            &report_details.join("  ·  "), false);
        header_y -= 15.0;
    }

    let mut image_heading = Vec::new();
    if options.include_sequence_numbers {
        image_heading.push(format!("Image {} / {}", image_index + 1, selected_count));
    }
    if options.include_file_names {
        image_heading.push(photo.name.clone());
    }
    if !image_heading.is_empty() {
        draw_text(&mut page, MARGIN, header_y, 10.0, Rgb(0.10, 0.19, 0.31),
            &shorten(&image_heading.join("  ·  "), 105), true);
        header_y -= 15.0;
    }
    if options.include_descriptions && !photo.description.trim().is_empty() {
        draw_text(&mut page, MARGIN, header_y, 8.3, Rgb(0.35, 0.40, 0.47),
            &shorten(&format!("Description : {}", photo.description.trim()), 112), false);
        header_y -= 13.0;
    }
    if let Some(warning) = photo.warning.as_deref().filter(|warning| !warning.trim().is_empty()) {
        draw_text(&mut page, MARGIN, header_y, 8.2, Rgb(0.72, 0.20, 0.15),
            &shorten(&format!("Avertissement : {}", warning.trim()), 112), true);
        header_y -= 13.0;
    }
    if photo.include_timestamp {
        if let Some(timestamp) = photo.timestamp.as_deref() {
            draw_text(&mut page, MARGIN, header_y, 8.0, Rgb(0.35, 0.40, 0.47),
                &format!("Date de l'image : {timestamp}"), false);
            header_y -= 12.0;
        }
    }

    let image_top = (header_y - 8.0).min(725.0);
    let max_image_height = 295.0;
    let image_scale = (TABLE_WIDTH / photo.width as f64)
        .min(max_image_height / photo.height as f64);
    let draw_width = photo.width as f64 * image_scale;
    let draw_height = photo.height as f64 * image_scale;
    let image_x = (PAGE_WIDTH - draw_width) * 0.5;
    let image_y = image_top - draw_height;
    push_ascii(&mut page, &format!(
        "q\n{draw_width:.3} 0 0 {draw_height:.3} {image_x:.3} {image_y:.3} cm\n/Im0 Do\nQ\n"));
    push_ascii(&mut page, &format!("0.65 0.69 0.74 RG 0.7 w {image_x:.3} {image_y:.3} {draw_width:.3} {draw_height:.3} re S\n"));
    draw_footer(&mut page, pages.len() + 1);
    pages.push(FolderPage { content: page, image_index: Some(image_index) });
}

fn render_group_overview(
    pages: &mut Vec<FolderPage>,
    options: &FolderReportOptions,
    photo: &FolderReportPhoto,
    image_index: usize,
    selected_count: usize,
    groups: &[(usize, ReportGroup)],
) {
    const GROUPS_PER_OVERVIEW_PAGE: usize = 8;
    let page_count = groups.len().max(1).div_ceil(GROUPS_PER_OVERVIEW_PAGE);
    for page_index in 0..page_count {
        let first_group = page_index * GROUPS_PER_OVERVIEW_PAGE;
        let page_groups = &groups[first_group..groups.len().min(first_group + GROUPS_PER_OVERVIEW_PAGE)];
        let mut page = Vec::new();
        let mut header_y = 807.0;
        if options.include_title && !options.title.trim().is_empty() {
            draw_text(&mut page, MARGIN, header_y, 16.0, Rgb(0.10, 0.19, 0.31),
                &shorten(options.title.trim(), 90), true);
            header_y -= 19.0;
        }
        let heading = if page_index == 0 {
            format!("Vue générale des groupes · image {} / {}", image_index + 1, selected_count)
        } else {
            format!("Vue générale des groupes · image {} / {} · suite {}", image_index + 1, selected_count, page_index + 1)
        };
        draw_text(&mut page, MARGIN, header_y, 10.0, Rgb(0.10, 0.19, 0.31),
            &shorten(&heading, 105), true);
        header_y -= 14.0;
        draw_text(&mut page, MARGIN, header_y, 8.5, Rgb(0.35, 0.40, 0.47),
            &shorten(&photo.name, 110), false);
        header_y -= 13.0;
        draw_text(&mut page, MARGIN, header_y, 7.8, Rgb(0.35, 0.40, 0.47),
            "Type/caractéristique; au-delà de 6, coupures récursives par moitiés sur l’axe X/Y le plus dispersé des centres.",
            false);
        header_y -= 14.0;
        if let Some(notice) = photo.series_notice.as_deref() {
            draw_text(&mut page, MARGIN, header_y, 7.8, Rgb(0.55, 0.29, 0.06),
                &shorten(notice, 115), false);
            header_y -= 11.0;
        }

        let max_image_height = 300.0;
        let image_scale = (TABLE_WIDTH / photo.width as f64)
            .min(max_image_height / photo.height as f64);
        let draw_width = photo.width as f64 * image_scale;
        let draw_height = photo.height as f64 * image_scale;
        let image_x = (PAGE_WIDTH - draw_width) * 0.5;
        let image_top = (header_y - 8.0).min(725.0);
        let image_y = image_top - draw_height;
        push_ascii(&mut page, &format!(
            "q\n{draw_width:.3} 0 0 {draw_height:.3} {image_x:.3} {image_y:.3} cm\n/Im0 Do\nQ\n"));
        push_ascii(&mut page, &format!(
            "0.65 0.69 0.74 RG 0.7 w {image_x:.3} {image_y:.3} {draw_width:.3} {draw_height:.3} re S\n"));

        let mut occupied_markers = Vec::new();
        for (group_number, group) in page_groups {
            let measurements = photo.session.measurements.iter()
                .filter(|measurement| group.measurement_ids.contains(&measurement.id))
                .collect::<Vec<_>>();
            if measurements.is_empty() { continue; }
            draw_group_region(
                &mut page, &measurements, *group_number, image_x, image_y, draw_width, draw_height,
                photo.width, photo.height,
            );
            draw_group_marker(
                &mut page, &measurements, *group_number, image_x, image_y, draw_width, draw_height,
                photo.width, photo.height, &group.description, &mut occupied_markers,
            );
        }

        let legend_top = image_y - 18.0;
        draw_text(&mut page, MARGIN, legend_top, 10.0, Rgb(0.10, 0.19, 0.31),
            "Index des groupes sur cette page", true);
        let first_legend_y = legend_top - 18.0;
        for (index, (group_number, group)) in page_groups.iter().enumerate() {
            let y = first_legend_y - index as f64 * 13.0;
            let color = group_color(*group_number);
            push_ascii(&mut page, &format!("{} {} {} rg\n", color.0, color.1, color.2));
            draw_circle(&mut page, MARGIN + 5.0, y + 2.0, 3.0, true);
            let counts = if group.measurement_ids.is_empty() {
                format!("{} cote(s), sans mesure", group.dimension_indices.len())
            } else {
                format!("{} mesure(s)", group.measurement_ids.len())
            };
            draw_text(&mut page, MARGIN + 12.0, y, 7.1, Rgb(0.12, 0.16, 0.21),
                &shorten(&format!("Groupe {group_number} · {} · {counts}", group.description), 105), false);
        }
        draw_footer(&mut page, pages.len() + 1);
        pages.push(FolderPage { content: page, image_index: Some(image_index) });
    }
}

// Small groups keep the annotations legible; oversized categories are split by image position.
const MAX_GROUP_MEASUREMENTS: usize = 6;

#[derive(Default)]
struct ReportGroup {
    description: String,
    measurement_ids: Vec<u64>,
    dimension_indices: Vec<usize>,
    annotation_indices: Vec<usize>,
}

fn build_report_groups(photo: &FolderReportPhoto) -> Vec<ReportGroup> {
    let mut groups = Vec::<ReportGroup>::new();
    if let Some(reference) = &photo.reference {
        for (dimension_index, dimension) in reference.dimensions.iter().enumerate() {
            let Some(measurement_id) = dimension.measurement_id else { continue; };
            if !photo.session.measurements.iter()
                .any(|measurement| measurement.id == measurement_id && !measurement.is_construction)
            {
                continue;
            }
            let description = group_characteristic(&dimension.label)
                .or_else(|| {
                    dimension.measurement_id
                        .and_then(|measurement_id| photo.session.measurements.iter()
                            .find(|measurement| measurement.id == measurement_id))
                        .map(|measurement| measurement.kind.label().to_owned())
                })
                .unwrap_or_else(|| "Autres cotes de référence".to_owned());
            let group_index = report_group_index(&mut groups, &description);
            groups[group_index].dimension_indices.push(dimension_index);
            if let Some(measurement_id) = dimension.measurement_id {
                if photo.session.measurements.iter()
                    .any(|measurement| measurement.id == measurement_id)
                {
                    push_unique(&mut groups[group_index].measurement_ids, measurement_id);
                }
            }
        }
    }

    for measurement in photo.session.measurements.iter().filter(|measurement| !measurement.is_construction) {
        let already_grouped = groups.iter()
            .any(|group| group.measurement_ids.contains(&measurement.id));
        if already_grouped { continue; }
        let description = group_characteristic(&measurement.label)
            .unwrap_or_else(|| measurement.kind.label().to_owned());
        let group_index = report_group_index(&mut groups, &description);
        push_unique(&mut groups[group_index].measurement_ids, measurement.id);
    }

    if groups.is_empty() {
        for measurement in photo.session.measurements.iter().filter(|measurement| !measurement.is_construction) {
            let description = measurement.kind.label().to_owned();
            let group_index = report_group_index(&mut groups, &description);
            push_unique(&mut groups[group_index].measurement_ids, measurement.id);
        }
    }

    let mut groups = groups.into_iter()
        .flat_map(|group| split_report_group(group, photo))
        .collect::<Vec<_>>();

    for (annotation_index, annotation) in photo.session.annotations.iter().enumerate() {
        let target_measurement_id = match &annotation.target {
            crate::geometry::AnnotationTarget::Measurement { measurement_id } => Some(*measurement_id),
            _ => None,
        };
        let containing_group = target_measurement_id.and_then(|id| groups.iter()
            .position(|group| group.measurement_ids.contains(&id)));
        let target_point = match &annotation.target {
            crate::geometry::AnnotationTarget::Measurement { measurement_id } => photo.session.measurements
                .iter().find(|measurement| measurement.id == *measurement_id)
                .map(measurement_center),
            crate::geometry::AnnotationTarget::Point { point } => Some((point.x, point.y)),
            crate::geometry::AnnotationTarget::Zone { first, second } =>
                Some(((first.x + second.x) * 0.5, (first.y + second.y) * 0.5)),
        };
        let nearest_group = target_point.and_then(|(target_x, target_y)| groups.iter().enumerate()
            .filter(|(_, group)| !group.measurement_ids.is_empty())
            .min_by(|(_, left), (_, right)| {
                let left_center = report_group_center(left, photo);
                let right_center = report_group_center(right, photo);
                let left_distance = (left_center.0 - target_x).hypot(left_center.1 - target_y);
                let right_distance = (right_center.0 - target_x).hypot(right_center.1 - target_y);
                left_distance.total_cmp(&right_distance)
            })
            .map(|(index, _)| index));
        if let Some(group_index) = containing_group.or(nearest_group).or_else(|| (!groups.is_empty()).then_some(0)) {
            groups[group_index].annotation_indices.push(annotation_index);
        }
    }
    groups
}

fn report_group_center(group: &ReportGroup, photo: &FolderReportPhoto) -> (f64, f64) {
    let centers = photo.session.measurements.iter()
        .filter(|measurement| group.measurement_ids.contains(&measurement.id))
        .map(measurement_center)
        .collect::<Vec<_>>();
    if centers.is_empty() { return (0.0, 0.0); }
    (
        centers.iter().map(|center| center.0).sum::<f64>() / centers.len() as f64,
        centers.iter().map(|center| center.1).sum::<f64>() / centers.len() as f64,
    )
}

fn report_group_index(groups: &mut Vec<ReportGroup>, description: &str) -> usize {
    if let Some(index) = groups.iter().position(|group| group.description == description) {
        return index;
    }
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
            .map(|row| format!("{base} · rangée {row}"))
            .unwrap_or_else(|| base.to_owned()));
    }
    if normalized.contains("pas y") {
        let base = group_label_prefix(label);
        return Some(number_after_marker(label, &["colonne"])
            .map(|column| format!("{base} · colonne {column}"))
            .unwrap_or_else(|| base.to_owned()));
    }
    if normalized.contains("décalage x") || normalized.contains("decalage x") {
        return Some(group_label_prefix(label).to_owned());
    }
    if normalized.contains("décalage y") || normalized.contains("decalage y") {
        return Some(group_label_prefix(label).to_owned());
    }
    if normalized.contains("hors tout x") {
        return Some("Dimensions hors tout X".to_owned());
    }
    if normalized.contains("hors tout y") {
        return Some("Dimensions hors tout Y".to_owned());
    }
    if normalized.contains("hors tout") {
        return Some("Dimensions hors tout".to_owned());
    }
    if normalized.contains("enveloppe motifs x") {
        return Some("Enveloppe des motifs X".to_owned());
    }
    if normalized.contains("enveloppe motifs y") {
        return Some("Enveloppe des motifs Y".to_owned());
    }
    if normalized.contains("enveloppe") {
        return Some("Enveloppe des motifs".to_owned());
    }
    if normalized.contains("motif") || normalized.contains("logement")
        || normalized.contains("profondeur")
    {
        return Some(group_label_prefix(label).to_owned());
    }
    None
}

fn group_label_prefix(label: &str) -> &str {
    [" — ", " – ", " - "].iter()
        .find_map(|&separator| label.find(separator).map(|index| label[..index].trim()))
        .filter(|prefix| !prefix.is_empty())
        .unwrap_or_else(|| label.trim())
}

fn number_after_marker(text: &str, markers: &[&str]) -> Option<String> {
    let normalized = text.to_lowercase();
    for marker in markers {
        let Some(marker_index) = normalized.find(marker) else { continue; };
        let digits = normalized[marker_index + marker.len()..].chars()
            .skip_while(|character| !character.is_ascii_digit())
            .take_while(|character| character.is_ascii_digit())
            .collect::<String>();
        if !digits.is_empty() { return Some(digits); }
    }
    None
}

pub(crate) fn short_group_marker_characteristic(description: &str) -> String {
    let (base, zone) = description.rsplit_once(" · zone ")
        .filter(|(_, zone)| !zone.is_empty() && zone.chars().all(|character| character.is_ascii_digit()))
        .map(|(base, zone)| (base, Some(zone)))
        .unwrap_or((description, None));
    let characteristic = match base {
        "Dimensions hors tout X" => "Hors tout X".to_owned(),
        "Dimensions hors tout Y" => "Hors tout Y".to_owned(),
        "Dimensions hors tout" => "Hors tout".to_owned(),
        "Enveloppe des motifs X" => "Enveloppe X".to_owned(),
        "Enveloppe des motifs Y" => "Enveloppe Y".to_owned(),
        "Enveloppe des motifs" => "Enveloppe".to_owned(),
        _ => {
            if let Some((pitch, row)) = base.rsplit_once(" · rangée ") {
                format!("{} · R{row}", shorten(pitch, 16))
            } else if let Some((pitch, column)) = base.rsplit_once(" · colonne ") {
                format!("{} · C{column}", shorten(pitch, 16))
            } else {
                shorten(base, 19)
            }
        }
    };
    zone.map(|zone| format!("{characteristic} · Z{zone}"))
        .unwrap_or(characteristic)
}

fn split_report_group(group: ReportGroup, photo: &FolderReportPhoto) -> Vec<ReportGroup> {
    let measurements = photo.session.measurements.iter()
        .filter(|measurement| group.measurement_ids.contains(&measurement.id))
        .collect::<Vec<_>>();
    if measurements.len() <= MAX_GROUP_MEASUREMENTS {
        return vec![group];
    }

    let zones = split_measurements_spatially(measurements, MAX_GROUP_MEASUREMENTS);
    let mut split_groups = Vec::with_capacity(zones.len() + 1);
    let mut unmatched_dimensions = Vec::new();
    for (zone_index, measurement_ids) in zones.into_iter().enumerate() {
        let mut zone = ReportGroup {
            description: format!("{} · zone {}", group.description, zone_index + 1),
            measurement_ids,
            dimension_indices: Vec::new(),
            annotation_indices: Vec::new(),
        };
        for dimension_index in &group.dimension_indices {
            let measurement_id = photo.reference.as_ref()
                .and_then(|reference| reference.dimensions.get(*dimension_index))
                .and_then(|dimension| dimension.measurement_id);
            if measurement_id.is_some_and(|id| zone.measurement_ids.contains(&id)) {
                zone.dimension_indices.push(*dimension_index);
            }
        }
        split_groups.push(zone);
    }

    for dimension_index in group.dimension_indices {
        let measurement_id = photo.reference.as_ref()
            .and_then(|reference| reference.dimensions.get(dimension_index))
            .and_then(|dimension| dimension.measurement_id);
        if measurement_id.is_none_or(|id| !group.measurement_ids.contains(&id)) {
            unmatched_dimensions.push(dimension_index);
        }
    }
    if !unmatched_dimensions.is_empty() {
        split_groups.push(ReportGroup {
            description: format!("{} · cotes sans mesure associée", group.description),
            measurement_ids: Vec::new(),
            dimension_indices: unmatched_dimensions,
            annotation_indices: Vec::new(),
        });
    }
    split_groups
}

fn split_measurements_spatially<'a>(
    mut measurements: Vec<&'a Measurement>,
    maximum: usize,
) -> Vec<Vec<u64>> {
    if measurements.len() <= maximum {
        return vec![measurements.iter().map(|measurement| measurement.id).collect()];
    }

    // Repeatedly split along the widest image axis, then keep the resulting zones in scan order.
    let centers = measurements.iter().map(|measurement| measurement_center(measurement))
        .collect::<Vec<_>>();
    let min_x = centers.iter().map(|center| center.0).fold(f64::INFINITY, f64::min);
    let max_x = centers.iter().map(|center| center.0).fold(f64::NEG_INFINITY, f64::max);
    let min_y = centers.iter().map(|center| center.1).fold(f64::INFINITY, f64::min);
    let max_y = centers.iter().map(|center| center.1).fold(f64::NEG_INFINITY, f64::max);
    let split_x = max_x - min_x >= max_y - min_y;
    measurements.sort_by(|left, right| {
        let left_center = measurement_center(left);
        let right_center = measurement_center(right);
        let primary = if split_x {
            left_center.0.total_cmp(&right_center.0)
        } else {
            left_center.1.total_cmp(&right_center.1)
        };
        primary.then_with(|| {
            let secondary = if split_x {
                left_center.1.total_cmp(&right_center.1)
            } else {
                left_center.0.total_cmp(&right_center.0)
            };
            secondary.then_with(|| left.id.cmp(&right.id))
        })
    });
    let right = measurements.split_off(measurements.len() / 2);
    let mut zones = split_measurements_spatially(measurements, maximum);
    zones.extend(split_measurements_spatially(right, maximum));
    zones
}

fn measurement_center(measurement: &Measurement) -> (f64, f64) {
    if measurement.points.is_empty() { return (0.0, 0.0); }
    let count = measurement.points.len() as f64;
    (
        measurement.points.iter().map(|point| point.x).sum::<f64>() / count,
        measurement.points.iter().map(|point| point.y).sum::<f64>() / count,
    )
}

fn render_folder_group_photo(
    pages: &mut Vec<FolderPage>,
    options: &FolderReportOptions,
    photo: &FolderReportPhoto,
    image_index: usize,
    selected_count: usize,
    group_number: usize,
    group: &ReportGroup,
    reference: Option<&FolderReportReference>,
) {
    let measurements = photo.session.measurements.iter()
        .filter(|measurement| group.measurement_ids.contains(&measurement.id))
        .collect::<Vec<_>>();
    let mut page = Vec::new();
    let mut header_y = 807.0;
    if options.include_title && !options.title.trim().is_empty() {
        draw_text(&mut page, MARGIN, header_y, 16.0, Rgb(0.10, 0.19, 0.31),
            &shorten(options.title.trim(), 90), true);
        header_y -= 19.0;
    }
    if options.include_folder_name {
        draw_text(&mut page, MARGIN, header_y, 8.5, Rgb(0.17, 0.22, 0.29),
            &format!("Dossier : {}", options.folder_name), false);
        header_y -= 14.0;
    }
    let mut report_details = Vec::new();
    if options.include_picture_count {
        report_details.push(format!("{} image(s)", selected_count));
    }
    if options.include_generation_date {
        report_details.push(format!("Généré le {}", utc_timestamp()));
    }
    if !report_details.is_empty() {
        draw_text(&mut page, MARGIN, header_y, 8.5, Rgb(0.17, 0.22, 0.29),
            &report_details.join("  ·  "), false);
        header_y -= 15.0;
    }
    let mut image_heading = Vec::new();
    if options.include_sequence_numbers {
        image_heading.push(format!("Image {} / {}", image_index + 1, selected_count));
    }
    if options.include_file_names { image_heading.push(photo.name.clone()); }
    if !image_heading.is_empty() {
        draw_text(&mut page, MARGIN, header_y, 10.0, Rgb(0.10, 0.19, 0.31),
            &shorten(&image_heading.join("  ·  "), 105), true);
        header_y -= 15.0;
    }
    if options.include_descriptions && !photo.description.trim().is_empty() {
        draw_text(&mut page, MARGIN, header_y, 8.3, Rgb(0.35, 0.40, 0.47),
            &shorten(&format!("Description : {}", photo.description.trim()), 112), false);
        header_y -= 13.0;
    }
    if let Some(warning) = photo.warning.as_deref().filter(|warning| !warning.trim().is_empty()) {
        draw_text(&mut page, MARGIN, header_y, 8.2, Rgb(0.72, 0.20, 0.15),
            &shorten(&format!("Avertissement : {}", warning.trim()), 112), true);
        header_y -= 13.0;
    }
    if photo.include_timestamp {
        if let Some(timestamp) = photo.timestamp.as_deref() {
            draw_text(&mut page, MARGIN, header_y, 8.0, Rgb(0.35, 0.40, 0.47),
                &format!("Date de l'image : {timestamp}"), false);
            header_y -= 12.0;
        }
    }
    let group_detail = format!(
        "Groupe {group_number} · {} · {} mesure(s) · {} cote(s) · {}",
        group.description,
        measurements.len(),
        group.dimension_indices.len(),
        photo.session.protocol.as_ref().map(|protocol|
            format!("{} v{}", protocol.name, protocol.version))
            .unwrap_or_else(|| "sans gabarit versionné".to_owned()),
    );
    draw_text(&mut page, MARGIN, header_y, 9.0, group_color(group_number),
        &shorten(&group_detail, 115), true);
    header_y -= 14.0;
    if let Some(notice) = photo.series_notice.as_deref() {
        draw_text(&mut page, MARGIN, header_y, 7.8, Rgb(0.55, 0.29, 0.06),
            &shorten(notice, 115), false);
        header_y -= 11.0;
    }
    if let Some(reference) = reference {
        let dimensions = group.dimension_indices.iter()
            .filter_map(|index| reference.dimensions.get(*index))
            .collect::<Vec<_>>();
        let status = if measurements.iter().any(|measurement| measurement.origin.eq_ignore_ascii_case("vision")) {
            "Verdict suspendu : mesures vision".to_owned()
        } else if reference.verdict_notice.is_some() {
            "Verdict suspendu : qualité ou comparabilité insuffisante".to_owned()
        } else {
            group_reference_status(&dimensions)
        };
        if let Some(notice) = reference.verdict_notice.as_deref() {
            draw_text(&mut page, MARGIN, header_y, 7.8, Rgb(0.55, 0.29, 0.06),
                &shorten(notice, 115), false);
            header_y -= 11.0;
        }
        draw_text(&mut page, MARGIN, header_y, 8.0, reference_status_color(&status),
            &shorten(&format!("Résultat du groupe : {status}"), 115), true);
        header_y -= 12.0;
    }

    let image_top = (header_y - 8.0).min(725.0);
    let max_image_height = 295.0;
    let image_scale = (TABLE_WIDTH / photo.width as f64)
        .min(max_image_height / photo.height as f64);
    let draw_width = photo.width as f64 * image_scale;
    let draw_height = photo.height as f64 * image_scale;
    let image_x = (PAGE_WIDTH - draw_width) * 0.5;
    let image_y = image_top - draw_height;
    push_ascii(&mut page, &format!(
        "q\n{draw_width:.3} 0 0 {draw_height:.3} {image_x:.3} {image_y:.3} cm\n/Im0 Do\nQ\n"));
    push_ascii(&mut page, &format!(
        "0.65 0.69 0.74 RG 0.7 w {image_x:.3} {image_y:.3} {draw_width:.3} {draw_height:.3} re S\n"));
    draw_group_region(
        &mut page,
        &measurements,
        group_number,
        image_x,
        image_y,
        draw_width,
        draw_height,
        photo.width,
        photo.height,
    );
    let mut occupied_markers = draw_measurement_overlays(
        &mut page,
        &measurements,
        image_x,
        image_y,
        draw_width,
        draw_height,
        photo.width,
        photo.height,
    );
    draw_group_marker(
        &mut page,
        &measurements,
        group_number,
        image_x,
        image_y,
        draw_width,
        draw_height,
        photo.width,
        photo.height,
        &group.description,
        &mut occupied_markers,
    );

    let mut page_image = Some(image_index);
    let mut page_number = pages.len() + 1;
    let heading_y = image_y - 18.0;
    draw_text(&mut page, MARGIN, heading_y, 11.0, Rgb(0.10, 0.19, 0.31),
        &format!("Mesures et comparaison du Groupe {group_number}"), true);
    let rows = build_group_comparison_rows(group, photo, reference);
    let mut cursor_y = draw_group_comparison_header(&mut page, heading_y - 8.0);
    for (row_index, row) in rows.iter().enumerate() {
        let row_height = row.height();
        if cursor_y - row_height < 42.0 {
            draw_footer(&mut page, page_number);
            pages.push(FolderPage { content: page, image_index: page_image.take() });
            page = Vec::new();
            page_number = pages.len() + 1;
            draw_text(&mut page, MARGIN, 808.0, 14.0, Rgb(0.10, 0.19, 0.31),
                &format!("Groupe {group_number} · tableau (suite)"), true);
            draw_text(&mut page, MARGIN, 789.0, 8.2, Rgb(0.35, 0.40, 0.47),
                &shorten(&group.description, 110), false);
            cursor_y = draw_group_comparison_header(&mut page, 765.0);
        }
        draw_group_comparison_row(&mut page, cursor_y, row_height, row, row_index);
        cursor_y -= row_height;
    }
    draw_footer(&mut page, page_number);
    pages.push(FolderPage { content: page, image_index: page_image });
}

struct GroupComparisonRow {
    cells: [Vec<String>; 6],
    status_color: Rgb,
    measurement_color: Rgb,
}

impl GroupComparisonRow {
    fn height(&self) -> f64 {
        let lines = self.cells.iter().map(Vec::len).max().unwrap_or(1);
        (lines as f64 * 8.5 + 8.0).max(26.0)
    }
}

const GROUP_COLUMN_WIDTHS: [f64; 6] = [145.0, 58.0, 86.0, 72.0, 72.0, 90.0];

fn build_group_comparison_rows(
    group: &ReportGroup,
    photo: &FolderReportPhoto,
    reference: Option<&FolderReportReference>,
) -> Vec<GroupComparisonRow> {
    let mut rows = Vec::new();
    let mut represented_measurements = Vec::<u64>::new();
    if let Some(reference) = reference {
        for dimension_index in &group.dimension_indices {
            let Some(dimension) = reference.dimensions.get(*dimension_index) else { continue; };
            let measurement = dimension.measurement_id.and_then(|id| {
                photo.session.measurements.iter().find(|measurement| measurement.id == id)
            });
            if let Some(measurement) = measurement {
                push_unique(&mut represented_measurements, measurement.id);
            }
            let measurement_label = measurement.map(|measurement| {
                let label = measurement.label.trim();
                let protocol_suffix = measurement_protocol_suffix(measurement, &photo.session);
                if label.is_empty() || label == format!("M{}", measurement.id) {
                    format!("M{}{}", measurement.id, protocol_suffix)
                } else {
                    format!("M{} · {}{}", measurement.id, label, protocol_suffix)
                }
            }).or_else(|| dimension.measurement_id.map(|id| format!("M{id}")))
                .unwrap_or_else(|| "—".to_owned());
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
            let measurement_label = source_label.map(|source| format!("{source} · {measurement_label}"))
                .unwrap_or(measurement_label);
            let measured = dimension.measured_mm.or_else(|| measurement
                .map(geometry::result)
                .and_then(|value| numeric_result_mm(&value)));
            let uncertainty = dimension.expanded_uncertainty_mm.or_else(|| {
                measurement.and_then(|measurement| measurement.expanded_uncertainty_mm)
            });
            let measured_label = measured.map(|value| uncertainty
                .map(|uncertainty| format!("{value:.3} ± {uncertainty:.3} mm"))
                .unwrap_or_else(|| format!("{value:.3} mm · U inconnue")))
                .or_else(|| measurement.map(geometry::result))
                .unwrap_or_else(|| "—".to_owned());
            let deviation = measured.map(|value| format!("{:+.3} mm", value - dimension.nominal_mm))
                .unwrap_or_else(|| "—".to_owned());
            let status = if is_vision_measurement(&photo.session, dimension.measurement_id) {
                "Verdict suspendu".to_owned()
            } else { suspended_status(&dimension.status, reference.verdict_notice.is_some()) };
            rows.push(group_comparison_row(
                &measurement_label,
                Some(dimension.nominal_mm),
                Some(dimension.tolerance_minus_mm),
                Some(dimension.tolerance_plus_mm),
                &measured_label,
                &deviation,
                &status,
                measurement.map(|measurement| measurement.id),
            ));
        }
    }

    for measurement in photo.session.measurements.iter()
        .filter(|measurement| group.measurement_ids.contains(&measurement.id))
    {
        if represented_measurements.contains(&measurement.id) { continue; }
        let id = format!("M{}", measurement.id);
        let label = measurement.label.trim();
        let protocol_suffix = measurement_protocol_suffix(measurement, &photo.session);
        let measurement_label = if label.is_empty() || label == id {
            format!("{id}{protocol_suffix}")
        } else {
            format!("{id} · {label}{protocol_suffix}")
        };
        rows.push(group_comparison_row(
            &measurement_label,
            None,
            None,
            None,
            &geometry::result(measurement),
            "—",
            if measurement.origin.eq_ignore_ascii_case("vision") { "Verdict suspendu" }
            else if reference.is_some() { "Non associée" } else { "Pas de référence" },
            Some(measurement.id),
        ));
    }
    rows
}

fn group_comparison_row(
    label: &str,
    nominal: Option<f64>,
    tolerance_minus: Option<f64>,
    tolerance_plus: Option<f64>,
    measured: &str,
    deviation: &str,
    status: &str,
    measurement_id: Option<u64>,
) -> GroupComparisonRow {
    let nominal = nominal.map(|value| format!("{value:.3} mm")).unwrap_or_else(|| "—".to_owned());
    let tolerance = match (tolerance_minus, tolerance_plus) {
        (Some(minus), Some(plus)) => format!("-{minus:.3}/+{plus:.3} mm"),
        _ => "—".to_owned(),
    };
    let values = [label.to_owned(), nominal, tolerance, measured.to_owned(),
        deviation.to_owned(), status.to_owned()];
    let cells = std::array::from_fn(|index| {
        let font_size = if index == 0 || index == 5 { 6.8 } else { 6.4 };
        let available = GROUP_COLUMN_WIDTHS[index] - 8.0;
        let max_chars = (available / (font_size * 0.52)).floor() as usize;
        limit_lines(wrap_text(&pdf_compatible_text(&values[index]), max_chars.max(1)), 3)
    });
    GroupComparisonRow {
        cells,
        status_color: reference_status_color(status),
        measurement_color: measurement_id.map(measurement_color).unwrap_or(Rgb(0.45, 0.49, 0.54)),
    }
}

fn numeric_result_mm(result: &str) -> Option<f64> {
    let value = result.strip_suffix(" mm")?.trim().parse::<f64>().ok()?;
    value.is_finite().then_some(value)
}

fn measurement_protocol_suffix(measurement: &Measurement, session: &Session) -> String {
    let origin = if measurement.origin.eq_ignore_ascii_case("vision") { " · vision" } else { "" };
    let Some(version) = measurement.protocol_version else { return origin.to_owned(); };
    if measurement.protocol_id.as_deref() == session.protocol.as_ref().map(|protocol| protocol.id.as_str()) {
        format!(" · v{version}")
    } else {
        format!(" · autre protocole v{version}")
    }
}

fn suspended_status(status: &str, suspend: bool) -> String {
    if suspend && matches!(status, "Dans la tolérance" | "Hors tolérance") {
        "Verdict suspendu".to_owned()
    } else {
        status.to_owned()
    }
}

fn draw_group_comparison_header(page: &mut Vec<u8>, top: f64) -> f64 {
    let height = 26.0;
    let bottom = top - height;
    push_ascii(page, &format!(
        "0.12 0.22 0.35 rg {MARGIN:.2} {bottom:.2} {TABLE_WIDTH:.2} {height:.2} re f\n"));
    let headings = ["Mesure / cote", "Nominal", "Tolérances", "Mesuré", "Écart", "Statut"];
    let mut x = MARGIN;
    for (index, heading) in headings.iter().enumerate() {
        draw_text(page, x + 4.0, top - 16.0, 6.8, Rgb(1.0, 1.0, 1.0), heading, true);
        x += GROUP_COLUMN_WIDTHS[index];
        if index < headings.len() - 1 {
            push_ascii(page, &format!(
                "0.55 0.60 0.66 RG 0.4 w {x:.2} {bottom:.2} m {x:.2} {top:.2} l S\n"));
        }
    }
    bottom
}

fn draw_group_comparison_row(
    page: &mut Vec<u8>,
    top: f64,
    height: f64,
    row: &GroupComparisonRow,
    index: usize,
) {
    let bottom = top - height;
    let background = if index % 2 == 0 { "0.96 0.97 0.98" } else { "1 1 1" };
    push_ascii(page, &format!(
        "{background} rg {MARGIN:.2} {bottom:.2} {TABLE_WIDTH:.2} {height:.2} re f\n"));
    push_ascii(page, &format!(
        "0.78 0.81 0.84 RG 0.35 w {MARGIN:.2} {bottom:.2} {TABLE_WIDTH:.2} {height:.2} re S\n"));
    push_ascii(page, &format!(
        "{} {} {} rg\n", row.measurement_color.0, row.measurement_color.1, row.measurement_color.2,
    ));
    draw_circle(page, MARGIN + 7.0, top - 11.5, 3.0, true);
    let mut x = MARGIN;
    for (cell_index, lines) in row.cells.iter().enumerate() {
        let text_x = if cell_index == 0 { x + 14.0 } else { x + 4.0 };
        for (line_index, line) in lines.iter().enumerate() {
            draw_text(page, text_x, top - 10.0 - line_index as f64 * 8.5, 6.6,
                if cell_index == 5 { row.status_color } else { Rgb(0.12, 0.16, 0.21) },
                line, (cell_index == 0 || cell_index == 5) && line_index == 0);
        }
        x += GROUP_COLUMN_WIDTHS[cell_index];
        if cell_index < row.cells.len() - 1 {
            push_ascii(page, &format!(
                "0.78 0.81 0.84 RG 0.35 w {x:.2} {bottom:.2} m {x:.2} {top:.2} l S\n"));
        }
    }
}

fn group_color(group_number: usize) -> Rgb {
    PALETTE[(group_number.saturating_sub(1)) % PALETTE.len()]
}

fn draw_group_region(
    page: &mut Vec<u8>,
    measurements: &[&Measurement],
    group_number: usize,
    image_x: f64,
    image_y: f64,
    draw_width: f64,
    draw_height: f64,
    source_width: u32,
    source_height: u32,
) {
    let Some((min_x, max_x, min_y, max_y)) =
        group_source_bounds(measurements, source_width, source_height) else { return; };
    let scale_x = draw_width / source_width as f64;
    let scale_y = draw_height / source_height as f64;
    let x = image_x + min_x * scale_x;
    let y = image_y + (source_height as f64 - max_y) * scale_y;
    let width = ((max_x - min_x) * scale_x).max(2.0);
    let height = ((max_y - min_y) * scale_y).max(2.0);
    let color = group_color(group_number);
    push_ascii(page, &format!(
        "{} {} {} RG 1.5 w [4 3] 0 d {x:.3} {y:.3} {width:.3} {height:.3} re S [] 0 d\n",
        color.0, color.1, color.2,
    ));
}

fn group_source_bounds(
    measurements: &[&Measurement],
    source_width: u32,
    source_height: u32,
) -> Option<(f64, f64, f64, f64)> {
    let mut points = measurements.iter().flat_map(|measurement| &measurement.points)
        .filter(|point| point.x.is_finite() && point.y.is_finite());
    let first = points.next()?;
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (first.x, first.x, first.y, first.y);
    for point in points {
        min_x = min_x.min(point.x);
        max_x = max_x.max(point.x);
        min_y = min_y.min(point.y);
        max_y = max_y.max(point.y);
    }
    for measurement in measurements.iter().filter(|measurement| {
        measurement.kind == Tool::Circle && measurement.points.len() >= 2
    }) {
        let center = measurement.points[0];
        let radius = center.dist(measurement.points[1]);
        min_x = min_x.min(center.x - radius);
        max_x = max_x.max(center.x + radius);
        min_y = min_y.min(center.y - radius);
        max_y = max_y.max(center.y + radius);
    }
    let pad_x = source_width as f64 * 0.025;
    let pad_y = source_height as f64 * 0.025;
    Some((
        (min_x - pad_x).max(0.0),
        (max_x + pad_x).min(source_width as f64),
        (min_y - pad_y).max(0.0),
        (max_y + pad_y).min(source_height as f64),
    ))
}

fn draw_group_marker(
    page: &mut Vec<u8>,
    measurements: &[&Measurement],
    group_number: usize,
    image_x: f64,
    image_y: f64,
    draw_width: f64,
    draw_height: f64,
    source_width: u32,
    source_height: u32,
    group_description: &str,
    occupied: &mut Vec<(f64, f64, f64, f64)>,
) {
    let Some((min_x, max_x, min_y, max_y)) =
        group_source_bounds(measurements, source_width, source_height) else { return; };
    let scale_x = draw_width / source_width as f64;
    let scale_y = draw_height / source_height as f64;
    let region = (
        image_x + min_x * scale_x,
        image_y + (source_height as f64 - max_y) * scale_y,
        image_x + max_x * scale_x,
        image_y + (source_height as f64 - min_y) * scale_y,
    );
    let measurement_bounds = measurements.iter().map(|measurement| {
        measurement_report_bounds(
            measurement, image_x, image_y, scale_x, scale_y, source_height,
        )
    }).collect::<Vec<_>>();
    let characteristic = short_group_marker_characteristic(group_description);
    let label = format!("Groupe {group_number} · {characteristic}");
    let width = (label.chars().count() as f64 * 3.3 + 10.0).clamp(30.0, 136.0);
    let height = 14.0;
    let candidates = [
        (region.0, region.3 + 2.0),
        (region.2 - width, region.3 + 2.0),
        (region.0, region.1 - height - 2.0),
        (region.2 - width, region.1 - height - 2.0),
        (region.0 - width - 2.0, region.3 - height),
        (region.2 + 2.0, region.3 - height),
    ];
    let chosen = {
        let is_clear = |x: f64, y: f64| {
            let candidate = (x, y, x + width, y + height);
            candidate.0 >= image_x + 1.0 && candidate.2 <= image_x + draw_width - 1.0
                && candidate.1 >= image_y + 1.0 && candidate.3 <= image_y + draw_height - 1.0
                && measurement_bounds.iter().all(|bounds| !rects_overlap(candidate, *bounds))
                && occupied.iter().all(|other| !rects_overlap(candidate, *other))
        };
        let outside_region = candidates.into_iter().find(|(x, y)| {
            is_clear(*x, *y) && !rects_overlap((*x, *y, *x + width, *y + height), region)
        });
        outside_region.or_else(|| {
            const GRID_COLUMNS: usize = 12;
            const GRID_ROWS: usize = 8;
            let distance_to_region = |x: f64, y: f64| {
                let dx = (region.0 - (x + width)).max(0.0).max(x - region.2);
                let dy = (region.1 - (y + height)).max(0.0).max(y - region.3);
                dx.hypot(dy)
            };
            (0..GRID_ROWS).flat_map(|row| {
                (0..GRID_COLUMNS).map(move |column| (row, column))
            }).map(|(row, column)| {
                let x_range = (draw_width - width - 4.0).max(0.0);
                let y_range = (draw_height - height - 4.0).max(0.0);
                let x = image_x + 2.0 + column as f64 * x_range / (GRID_COLUMNS - 1) as f64;
                let y = image_y + 2.0 + row as f64 * y_range / (GRID_ROWS - 1) as f64;
                (x, y)
            }).filter(|(x, y)| is_clear(*x, *y))
                .min_by(|left, right| {
                    distance_to_region(left.0, left.1)
                        .total_cmp(&distance_to_region(right.0, right.1))
                })
        })
    };
    let Some((x, y)) = chosen else { return; };
    occupied.push((x, y, x + width, y + height));
    let color = group_color(group_number);
    push_ascii(page, &format!(
        "{} {} {} rg {x:.3} {y:.3} {width:.3} {height:.3} re f\n",
        color.0, color.1, color.2,
    ));
    draw_text(page, x + 4.0, y + 4.0, 5.7, group_marker_text_color(color), &label, true);
}

fn group_marker_text_color(background: Rgb) -> Rgb {
    let luminance = |channel: f64| {
        if channel <= 0.04045 { channel / 12.92 }
        else { ((channel + 0.055) / 1.055).powf(2.4) }
    };
    let luminance = 0.2126 * luminance(background.0)
        + 0.7152 * luminance(background.1)
        + 0.0722 * luminance(background.2);
    let white_contrast = 1.05 / (luminance + 0.05);
    let dark_contrast = (luminance + 0.05) / 0.05;
    if dark_contrast >= white_contrast { Rgb(0.06, 0.08, 0.11) }
    else { Rgb(1.0, 1.0, 1.0) }
}

fn rects_overlap(left: (f64, f64, f64, f64), right: (f64, f64, f64, f64)) -> bool {
    left.0 < right.2 && left.2 > right.0 && left.1 < right.3 && left.3 > right.1
}

fn group_reference_status(dimensions: &[&FolderReportReferenceDimension]) -> String {
    if dimensions.is_empty() {
        return "Aucune cote de référence associée à ce groupe".to_owned();
    }
    if dimensions.iter().any(|dimension| dimension.status == "Hors tolérance") {
        "Au moins une cote du groupe est hors tolérance".to_owned()
    } else if dimensions.iter().all(|dimension| dimension.status == "Dans la tolérance") {
        "Toutes les cotes du groupe comparées sont dans la tolérance".to_owned()
    } else {
        "Comparaison incomplète".to_owned()
    }
}

fn reference_status_color(status: &str) -> Rgb {
    let normalized = status.to_lowercase();
    if normalized.contains("hors tolérance") {
        Rgb(0.72, 0.16, 0.13)
    } else if normalized.starts_with("toutes") || normalized == "dans la tolérance" {
        Rgb(0.13, 0.48, 0.28)
    } else {
        Rgb(0.55, 0.29, 0.06)
    }
}

fn render_folder_reference(
    pages: &mut Vec<FolderPage>,
    reference: &FolderReportReference,
    session: &Session,
    image_indices: &[usize],
    comparisons_are_grouped: bool,
) {
    for (index, image_index) in image_indices.iter().copied().enumerate() {
        let mut page = Vec::new();
        draw_text(&mut page, MARGIN, 807.0, 14.0, Rgb(0.10, 0.19, 0.31),
            &shorten(&format!("Document de référence · {}", reference.source_name), 95), true);
        draw_text(&mut page, MARGIN, 789.0, 8.5, Rgb(0.35, 0.40, 0.47),
            &format!("Page {} / {} · document importé dans le projet", index + 1, image_indices.len()), false);
        let source = &reference.pages[index];
        let scale = (TABLE_WIDTH / source.width as f64).min(500.0 / source.height as f64);
        let width = source.width as f64 * scale;
        let height = source.height as f64 * scale;
        let x = (PAGE_WIDTH - width) * 0.5;
        let y = 275.0 + (500.0 - height) * 0.5;
        push_ascii(&mut page, &format!(
            "q\n{width:.3} 0 0 {height:.3} {x:.3} {y:.3} cm\n/Im0 Do\nQ\n"));
        draw_reference_ocr_boxes(
            &mut page, &reference.dimensions, index as u32 + 1,
            source.width, source.height, x, y, width, height,
        );
        push_ascii(&mut page, &format!("0.65 0.69 0.74 RG 0.7 w {x:.3} {y:.3} {width:.3} {height:.3} re S\n"));
        draw_footer(&mut page, pages.len() + 1);
        pages.push(FolderPage { content: page, image_index: Some(image_index) });
    }

    if comparisons_are_grouped { return; }

    let row_capacity = 20usize;
    let chunks = reference.dimensions.len().div_ceil(row_capacity).max(1);
    for chunk_index in 0..chunks {
        let mut page = Vec::new();
        draw_text(&mut page, MARGIN, 807.0, 14.0, Rgb(0.10, 0.19, 0.31),
            &shorten(&format!("Comparaison · {}", reference.source_name), 100), true);
        let vision_present = reference.dimensions.iter().any(|dimension| is_vision_measurement(session, dimension.measurement_id));
        let overall_status = if vision_present { "Verdict suspendu : mesures vision" } else { &reference.overall_status };
        let overall_color = if overall_status.starts_with("Toutes") {
            Rgb(0.13, 0.48, 0.28)
        } else if reference.overall_status.starts_with("Au moins") {
            Rgb(0.72, 0.16, 0.13)
        } else { Rgb(0.55, 0.29, 0.06) };
        draw_text(&mut page, MARGIN, 789.0, 8.3, overall_color,
            &shorten(&format!("Résultat pièce : {}", reference.overall_status), 100), true);
        draw_text(&mut page, MARGIN, 775.0, 7.8, Rgb(0.55, 0.29, 0.06),
            &shorten(reference.verdict_notice.as_deref().unwrap_or(
                "Décision par intervalle mesuré ± U; sans U élargie documentée, statut à confirmer.",
            ), 110), false);
        if let Some(error) = &reference.source_error {
            draw_text(&mut page, MARGIN, 761.0, 8.0, Rgb(0.72, 0.20, 0.15),
                &shorten(&format!("Document source non rendu : {error}"), 110), true);
        }
        let start = chunk_index * row_capacity;
        let end = (start + row_capacity).min(reference.dimensions.len());
        if start == end {
            draw_text(&mut page, MARGIN, 744.0, 9.0, Rgb(0.35, 0.40, 0.47),
                "Aucune cote avec tolérance n'a été détectée dans ce document.", false);
        } else {
            let mut y = 744.0;
            for (row_index, dimension) in reference.dimensions.iter().enumerate().take(end).skip(start) {
                let top = y;
                let bottom = top - 32.0;
                let shade = if row_index % 2 == 0 { "0.96 0.97 0.98" } else { "1 1 1" };
                push_ascii(&mut page, &format!("{shade} rg {MARGIN:.2} {bottom:.2} {TABLE_WIDTH:.2} 32 re f\n"));
                push_ascii(&mut page, &format!("0.78 0.81 0.84 RG 0.35 w {MARGIN:.2} {bottom:.2} {TABLE_WIDTH:.2} 32 re S\n"));
                draw_text(&mut page, MARGIN + 6.0, top - 12.0, 8.0, Rgb(0.12, 0.16, 0.21),
                    &shorten(&dimension.label, 80), true);
                let measured = dimension.measured_mm.map(|value| dimension.expanded_uncertainty_mm
                    .map(|uncertainty| format!("{value:.3} ± {uncertainty:.3} mm"))
                    .unwrap_or_else(|| format!("{value:.3} mm · U inconnue")))
                    .unwrap_or_else(|| "—".into());
                let source_page = dimension.ocr_page.map(|page| format!("Source p. {page} · "))
                    .unwrap_or_default();
                let detail = format!("{source_page}N {:.3} mm · tol. -{:.3}/+{:.3} mm · mesuré {measured}",
                    dimension.nominal_mm, dimension.tolerance_minus_mm, dimension.tolerance_plus_mm);
                draw_text(&mut page, MARGIN + 6.0, top - 25.0, 7.5, Rgb(0.35, 0.40, 0.47),
                    &shorten(&detail, 83), false);
                let status = if is_vision_measurement(session, dimension.measurement_id) { "Verdict suspendu" } else { &dimension.status };
                let status_color = if status == "Dans la tolérance" {
                    Rgb(0.13, 0.48, 0.28)
                } else if dimension.status == "Hors tolérance" {
                    Rgb(0.72, 0.16, 0.13)
                } else { Rgb(0.48, 0.38, 0.12) };
                draw_text(&mut page, PAGE_WIDTH - MARGIN - 105.0, top - 12.0, 7.5, status_color,
                    &shorten(&dimension.status, 20), true);
                y -= 35.0;
            }
        }
        draw_footer(&mut page, pages.len() + 1);
        pages.push(FolderPage { content: page, image_index: None });
    }
}

fn draw_reference_ocr_boxes(
    page: &mut Vec<u8>,
    dimensions: &[FolderReportReferenceDimension],
    page_number: u32,
    source_width: u32,
    source_height: u32,
    image_x: f64,
    image_y: f64,
    draw_width: f64,
    draw_height: f64,
) {
    for (index, dimension) in dimensions.iter().enumerate() {
        if dimension.ocr_page != Some(page_number) { continue; }
        let Some(bbox) = dimension.ocr_bbox.as_ref() else { continue; };
        let coordinate_width = if bbox.image_width == 0 { source_width } else { bbox.image_width };
        let coordinate_height = if bbox.image_height == 0 { source_height } else { bbox.image_height };
        if coordinate_width == 0 || coordinate_height == 0 { continue; }
        let scale_x = draw_width / coordinate_width as f64;
        let scale_y = draw_height / coordinate_height as f64;
        let left = (bbox.left as f64).min(coordinate_width as f64);
        let top = (bbox.top as f64).min(coordinate_height as f64);
        let right = (bbox.left.saturating_add(bbox.width) as f64).min(coordinate_width as f64);
        let bottom = (bbox.top.saturating_add(bbox.height) as f64).min(coordinate_height as f64);
        if right <= left || bottom <= top { continue; }
        let x = image_x + left * scale_x;
        let y = image_y + draw_height - bottom * scale_y;
        let width = (right - left) * scale_x;
        let height = (bottom - top) * scale_y;
        let color = group_color(index + 1);
        push_ascii(page, &format!(
            "{} {} {} RG 1.2 w {x:.3} {y:.3} {width:.3} {height:.3} re S\n",
            color.0, color.1, color.2,
        ));
        draw_text(page, x, y + height + 2.0, 6.5, color,
            &format!("OCR {}", dimension.id.strip_prefix("ocr-").unwrap_or("?")), true);
    }
}

fn assemble_folder_pdf(pages: &[FolderPage], images: &[ReportImage]) -> Vec<u8> {
    let image_start = 3 + pages.len() * 2;
    let regular_font_id = image_start + images.len();
    let bold_font_id = regular_font_id + 1;
    let last_id = bold_font_id;
    let mut objects: Vec<Option<Vec<u8>>> = vec![None; last_id + 1];

    objects[1] = Some(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    let kids = (0..pages.len()).map(|index| format!("{} 0 R", 3 + index * 2))
        .collect::<Vec<_>>().join(" ");
    objects[2] = Some(format!("<< /Type /Pages /Kids [{kids}] /Count {} >>", pages.len()).into_bytes());

    for (index, page) in pages.iter().enumerate() {
        let page_id = 3 + index * 2;
        let content_id = page_id + 1;
        let image_resource = page.image_index.map(|image_index| {
            format!("/XObject << /Im0 {} 0 R >>", image_start + image_index)
        }).unwrap_or_default();
        objects[page_id] = Some(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_WIDTH:.0} {PAGE_HEIGHT:.0}] \
             /Resources << /Font << /F1 {regular_font_id} 0 R /F2 {bold_font_id} 0 R >> {image_resource} >> \
             /Contents {content_id} 0 R >>"
        ).into_bytes());
        objects[content_id] = Some(stream_object(&format!("<< /Length {} >>", page.content.len()), &page.content));
    }

    for (index, image) in images.iter().enumerate() {
        let image_dict = format!(
            "<< /Type /XObject /Subtype /Image /Width {} /Height {} \
             /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode \
             /DecodeParms << /Predictor 15 /Colors 3 /BitsPerComponent 8 /Columns {} >> /Length {} >>",
            image.width, image.height, image.width, image.png_idat.len());
        objects[image_start + index] = Some(stream_object(&image_dict, &image.png_idat));
    }
    objects[regular_font_id] = Some(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_vec());
    objects[bold_font_id] = Some(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>".to_vec());

    let mut pdf = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = vec![0usize; last_id + 1];
    for id in 1..=last_id {
        offsets[id] = pdf.len();
        push_ascii(&mut pdf, &format!("{id} 0 obj\n"));
        pdf.extend_from_slice(objects[id].as_deref().unwrap_or_default());
        push_ascii(&mut pdf, "\nendobj\n");
    }
    let xref_offset = pdf.len();
    push_ascii(&mut pdf, &format!("xref\n0 {}\n0000000000 65535 f \n", last_id + 1));
    for offset in offsets.iter().skip(1) {
        push_ascii(&mut pdf, &format!("{offset:010} 00000 n \n"));
    }
    push_ascii(&mut pdf, &format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n", last_id + 1));
    pdf
}

fn encode_png_idat(rgb: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(rgb, width, height, image::ExtendedColorType::Rgb8)
        .map_err(|error| format!("Encodage de l'image du rapport impossible : {error}"))?;

    if png.get(..8) != Some(&b"\x89PNG\r\n\x1a\n"[..]) {
        return Err("En-tête PNG inattendu pendant la génération du rapport".into());
    }
    let mut idat = Vec::new();
    let mut offset = 8usize;
    while offset.checked_add(12).is_some_and(|end| end <= png.len()) {
        let chunk_len = u32::from_be_bytes(png[offset..offset + 4].try_into().unwrap()) as usize;
        let chunk_type = &png[offset + 4..offset + 8];
        let data_start = offset + 8;
        let data_end = data_start.checked_add(chunk_len)
            .ok_or_else(|| "Bloc PNG trop volumineux".to_owned())?;
        let chunk_end = data_end.checked_add(4)
            .ok_or_else(|| "Bloc PNG invalide".to_owned())?;
        if chunk_end > png.len() { return Err("Bloc PNG tronqué".into()); }
        if chunk_type == b"IDAT" { idat.extend_from_slice(&png[data_start..data_end]); }
        if chunk_type == b"IEND" { break; }
        offset = chunk_end;
    }
    if idat.is_empty() { return Err("Données PNG absentes dans le rapport".into()); }
    Ok(idat)
}

fn draw_construction_overlays(
    page: &mut Vec<u8>, photo: &FolderReportPhoto,
    image_x: f64, image_y: f64, draw_width: f64, draw_height: f64,
) {
    let guides = photo.session.measurements.iter()
        .filter(|measurement| measurement.is_construction).collect::<Vec<_>>();
    draw_measurement_overlays(page, &guides, image_x, image_y, draw_width, draw_height,
        photo.width, photo.height);
}

fn draw_measurement_overlays(
    page: &mut Vec<u8>,
    measurements: &[&Measurement],
    image_x: f64,
    image_y: f64,
    draw_width: f64,
    draw_height: f64,
    source_width: u32,
    source_height: u32,
) -> Vec<(f64, f64, f64, f64)> {
    let scale_x = draw_width / source_width as f64;
    let scale_y = draw_height / source_height as f64;
    let mut occupied_badges = Vec::new();
    let measurement_bounds = measurements.iter().map(|measurement| {
        measurement_report_bounds(
            measurement, image_x, image_y, scale_x, scale_y, source_height,
        )
    }).collect::<Vec<_>>();
    push_ascii(page, &format!(
        "q\n{image_x:.3} {image_y:.3} {draw_width:.3} {draw_height:.3} re W n\n"));

    for (measurement_index, measurement) in measurements.iter().enumerate() {
        let color = if measurement.is_construction { Rgb(0.451, 0.325, 0.729) } else { measurement_color(measurement.id) };
        let points: Vec<(f64, f64)> = measurement.points.iter()
            .map(|point| map_point(*point, image_x, image_y, scale_x, scale_y, source_height))
            .collect();
        push_ascii(page, &format!("{} {} {} RG {} {} {} rg 1.6 w 1 J 1 j\n",
            color.0, color.1, color.2, color.0, color.1, color.2));

        push_ascii(page, if measurement.is_construction { "[8 5] 0 d\n" } else { "[] 0 d\n" });
        match measurement.kind {
            Tool::ObjectCount => {}
            Tool::CircleFit | Tool::Ellipse | Tool::Arc => {
                if let Some(curve) = geometry::sampled_curve(measurement.kind, &measurement.points, 128) {
                    let curve = curve.iter().map(|point| map_point(*point, image_x, image_y, scale_x, scale_y, source_height)).collect::<Vec<_>>();
                    draw_polyline(page, &curve, measurement.kind != Tool::Arc);
                }
                if measurement.kind == Tool::CircleFit && !measurement.is_construction {
                    if let Some(circle) = geometry::fit_circle(&measurement.points) {
                        let center = map_point(circle.center, image_x, image_y, scale_x, scale_y, source_height);
                        let radius_x = circle.radius * scale_x;
                        let radius_y = circle.radius * scale_y;
                        draw_line(page, (center.0 - radius_x, center.1), (center.0 + radius_x, center.1));
                        draw_line(page, (center.0, center.1 - radius_y), (center.0, center.1 + radius_y));
                        draw_line(page, (center.0 - 3.0, center.1), (center.0 + 3.0, center.1));
                        draw_line(page, (center.0, center.1 - 3.0), (center.0, center.1 + 3.0));
                    }
                }
            }
            Tool::PointLineDistance if points.len() >= 3 => {
                draw_line(page, points[1], points[2]);
                let original = measurement.points[0];
                let first = measurement.points[1];
                let second = measurement.points[2];
                let axis = Point { x: second.x - first.x, y: second.y - first.y };
                let t = ((original.x - first.x) * axis.x + (original.y - first.y) * axis.y)
                    / (axis.x * axis.x + axis.y * axis.y);
                let projection = Point { x: first.x + axis.x * t, y: first.y + axis.y * t };
                draw_line(page, points[0], map_point(projection, image_x, image_y, scale_x, scale_y, source_height));
            }
            Tool::ParallelDistance if points.len() >= 4 => {
                draw_line(page, points[0], points[1]);
                draw_line(page, points[2], points[3]);
            }
            Tool::Coordinate if !points.is_empty() => {
                let (x, y) = points[0];
                draw_line(page, (x - 5.0, y), (x + 5.0, y));
                draw_line(page, (x, y - 5.0), (x, y + 5.0));
            }
            Tool::Circle if measurement.points.len() >= 2 => {
                let center = points[0];
                let rim = points[1];
                let radius = measurement.points[0].dist(measurement.points[1])
                    * (scale_x + scale_y) * 0.5;
                draw_circle(page, center.0, center.1, radius, false);
                if !measurement.is_construction { draw_line(page, center, rim); }
            }
            Tool::Rectangle if points.len() >= 2 => {
                let left = points[0].0.min(points[1].0);
                let bottom = points[0].1.min(points[1].1);
                let width = (points[0].0 - points[1].0).abs();
                let height = (points[0].1 - points[1].1).abs();
                push_ascii(page, &format!("{left:.3} {bottom:.3} {width:.3} {height:.3} re S\n"));
                if !measurement.is_construction {
                    let first_diagonal = (points[0].0, points[1].1);
                    let second_diagonal = (points[1].0, points[0].1);
                    draw_line(page, points[0], points[1]);
                    draw_line(page, first_diagonal, second_diagonal);
                    let center = ((points[0].0 + points[1].0) * 0.5,
                        (points[0].1 + points[1].1) * 0.5);
                    draw_line(page, (center.0 - 3.0, center.1), (center.0 + 3.0, center.1));
                    draw_line(page, (center.0, center.1 - 3.0), (center.0, center.1 + 3.0));
                }
            }
            Tool::Polygon if points.len() >= 3 => draw_polyline(page, &points, true),
            _ => draw_polyline(page, &points, false),
        }

        push_ascii(page, "[] 0 d\n");
        if measurement.is_construction { continue; }

        if !measurement.method.as_deref()
            .is_some_and(|method| method.starts_with("Détection géométrique guidée"))
        {
            for point in &points { draw_circle(page, point.0, point.1, 2.0, true); }
        }

        if !measurement.points.is_empty() {
            let reference = format!("M{}", measurement.id);
            let badge_width = (reference.chars().count() as f64 * 4.2 + 7.0)
                .clamp(20.0, 100.0)
                .min((draw_width - 2.0).max(1.0));
            let badge_height = 13.0;
            let bounds = measurement_bounds[measurement_index];
            let candidates = [
                (bounds.0, bounds.3 + 3.0),
                (bounds.2 - badge_width, bounds.3 + 3.0),
                (bounds.0, bounds.1 - badge_height - 3.0),
                (bounds.2 - badge_width, bounds.1 - badge_height - 3.0),
                (bounds.0 - badge_width - 3.0, bounds.1),
                (bounds.2 + 3.0, bounds.1),
            ];
            let badge = candidates.into_iter().find(|(x, y)| {
                let candidate = (*x, *y, *x + badge_width, *y + badge_height);
                candidate.0 >= image_x + 1.0
                    && candidate.2 <= image_x + draw_width - 1.0
                    && candidate.1 >= image_y + 1.0
                    && candidate.3 <= image_y + draw_height - 1.0
                    && measurement_bounds.iter().all(|region| !rects_overlap(candidate, *region))
                    && occupied_badges.iter().all(|other| !rects_overlap(candidate, *other))
            });
            if let Some((badge_x, badge_y)) = badge {
                occupied_badges.push((badge_x, badge_y, badge_x + badge_width, badge_y + badge_height));
                push_ascii(page, &format!(
                    "{} {} {} rg {badge_x:.3} {badge_y:.3} {badge_width:.3} {badge_height:.3} re f\n",
                    color.0, color.1, color.2));
                draw_text(page, badge_x + 3.0, badge_y + 3.5, 7.0,
                    Rgb(1.0, 1.0, 1.0), &reference, true);
            }
        }
    }
    push_ascii(page, "Q\n");
    occupied_badges
}

fn measurement_report_bounds(
    measurement: &Measurement,
    image_x: f64,
    image_y: f64,
    scale_x: f64,
    scale_y: f64,
    source_height: u32,
) -> (f64, f64, f64, f64) {
    let points = measurement.points.iter().map(|point| {
        map_point(*point, image_x, image_y, scale_x, scale_y, source_height)
    }).collect::<Vec<_>>();
    let Some(first) = points.first().copied() else {
        return (image_x, image_y, image_x, image_y);
    };
    let mut bounds = (
        points.iter().map(|point| point.0).fold(first.0, f64::min),
        points.iter().map(|point| point.1).fold(first.1, f64::min),
        points.iter().map(|point| point.0).fold(first.0, f64::max),
        points.iter().map(|point| point.1).fold(first.1, f64::max),
    );
    if let Some(curve) = geometry::sampled_curve(measurement.kind, &measurement.points, 128) {
        for point in curve {
            let mapped = map_point(point, image_x, image_y, scale_x, scale_y, source_height);
            bounds.0 = bounds.0.min(mapped.0);
            bounds.1 = bounds.1.min(mapped.1);
            bounds.2 = bounds.2.max(mapped.0);
            bounds.3 = bounds.3.max(mapped.1);
        }
    }
    (bounds.0 - 2.0, bounds.1 - 2.0, bounds.2 + 2.0, bounds.3 + 2.0)
}

fn map_point(point: Point, image_x: f64, image_y: f64, scale_x: f64,
    scale_y: f64, source_height: u32) -> (f64, f64) {
    (image_x + point.x * scale_x,
     image_y + (source_height as f64 - point.y) * scale_y)
}

fn draw_line(page: &mut Vec<u8>, a: (f64, f64), b: (f64, f64)) {
    push_ascii(page, &format!("{:.3} {:.3} m {:.3} {:.3} l S\n", a.0, a.1, b.0, b.1));
}

fn draw_polyline(page: &mut Vec<u8>, points: &[(f64, f64)], close: bool) {
    if let Some(first) = points.first() {
        push_ascii(page, &format!("{:.3} {:.3} m\n", first.0, first.1));
        for point in points.iter().skip(1) {
            push_ascii(page, &format!("{:.3} {:.3} l\n", point.0, point.1));
        }
        push_ascii(page, if close { "h S\n" } else { "S\n" });
    }
}

fn draw_circle(page: &mut Vec<u8>, x: f64, y: f64, radius: f64, fill: bool) {
    if !radius.is_finite() || radius <= 0.0 { return; }
    let k = radius * 0.552_284_749_8;
    push_ascii(page, &format!(
        "{:.3} {:.3} m {:.3} {:.3} {:.3} {:.3} {:.3} {:.3} c \
         {:.3} {:.3} {:.3} {:.3} {:.3} {:.3} c \
         {:.3} {:.3} {:.3} {:.3} {:.3} {:.3} c \
         {:.3} {:.3} {:.3} {:.3} {:.3} {:.3} c {}\n",
        x + radius, y,
        x + radius, y + k, x + k, y + radius, x, y + radius,
        x - k, y + radius, x - radius, y + k, x - radius, y,
        x - radius, y - k, x - k, y - radius, x, y - radius,
        x + k, y - radius, x + radius, y - k, x + radius, y,
        if fill { "h f" } else { "h S" }));
}

struct TableRow {
    cells: [Vec<String>; 5],
    color: Rgb,
}

impl TableRow {
    fn from_measurement(measurement: &Measurement, calibrations: &[crate::storage::Calibration]) -> Self {
        let color = measurement_color(measurement.id);
        let id_reference = format!("M{}", measurement.id);
        let label = measurement.label.trim();
        let reference = if label.is_empty() { id_reference.clone() } else { label.to_owned() };
        let reference_cell = if reference == id_reference {
            reference
        } else {
            format!("{}\nID : {}", reference, id_reference)
        };
        let calibration = measurement.mm_per_pixel
            .filter(|value| value.is_finite() && *value > 0.0)
            .map(|value| {
                let profile = measurement.calibration_id.as_ref()
                    .and_then(|id| calibrations.iter().find(|profile| &profile.id == id));
                match profile {
                    Some(profile) => {
                        let axes = match (profile.scale_x_mm_per_pixel, profile.scale_y_mm_per_pixel) {
                            (Some(x), Some(y)) => format!("X {x:.8} · Y {y:.8} mm/px"),
                            _ => format!("{value:.8} mm/px"),
                        };
                        let status = match profile.validation_status.as_str() {
                            "verified" => "contrôle indépendant validé",
                            "failed" => "contrôle indépendant échoué",
                            "unverified" => "contrôle à confirmer (U absent)",
                            _ => "référence unique · non vérifié",
                        };
                        let check = if !profile.corner_verification_residuals_mm.is_empty() {
                            format!(" · résidus coins [{}] mm", profile.corner_verification_residuals_mm.iter()
                                .map(|residual| format!("{residual:+.4}"))
                                .collect::<Vec<_>>().join(" / "))
                        } else {
                            profile.verification_residual_mm.map(|residual|
                                format!(" · résidu contrôle {residual:+.4} mm")).unwrap_or_default()
                        };
                        let reference_image = Path::new(&profile.reference_image).file_name()
                            .and_then(|name| name.to_str()).unwrap_or("non renseignée");
                        format!("{axes}\nRéf. {} · {:.4} mm / {:.2} px · {} · {}{} · auteur : {} · image : {} · {}×{} · {}",
                            profile.reference_type, profile.reference_mm, profile.reference_px, profile.origin,
                            status, check, profile.author, reference_image,
                            profile.resolution[0], profile.resolution[1],
                            utc_timestamp_at(profile.created_at_unix_secs))
                    }
                    None => format!("{value:.8} mm/px · provenance du profil absente"),
                }
            })
            .unwrap_or_else(|| "Non étalonné · pixels".into());
        let context = format!("Capture : {}\nProfil : {}\nOrigine : {}{}",
            default_dash(&measurement.capture_id),
            default_dash(measurement.calibration_id.as_deref().unwrap_or("")),
            measurement.origin,
            measurement.method.as_deref().filter(|method| !method.trim().is_empty())
                .map(|method| format!("\nMéthode : {method}")).unwrap_or_default());
        let values = [
            reference_cell,
            measurement.kind.label().to_owned(),
            geometry::result(measurement),
            calibration,
            context,
        ];
        let cells = std::array::from_fn(|index| {
            let max_chars = ((COLUMN_WIDTHS[index] - 11.0) / (7.5 * 0.52)).floor() as usize;
            let display_value = pdf_compatible_text(&values[index]);
            limit_lines(wrap_text(&display_value, max_chars.max(1)), 12)
        });
        Self { cells, color }
    }

    fn height(&self) -> f64 {
        let lines = self.cells.iter().map(Vec::len).max().unwrap_or(1);
        (lines as f64 * 9.0 + 8.0).max(25.0)
    }
}

fn draw_table_header(page: &mut Vec<u8>, top: f64) -> f64 {
    let height = 24.0;
    let bottom = top - height;
    push_ascii(page, &format!("0.12 0.22 0.35 rg {MARGIN:.2} {bottom:.2} {TABLE_WIDTH:.2} {height:.2} re f\n"));
    let headings = ["Référence", "Outil", "Résultat", "Étalonnage", "Capture / profil"];
    let mut x = MARGIN;
    for (index, heading) in headings.iter().enumerate() {
        draw_text(page, x + 5.0, top - 15.5, 8.0, Rgb(1.0, 1.0, 1.0), heading, true);
        x += COLUMN_WIDTHS[index];
        if index < headings.len() - 1 {
            push_ascii(page, &format!("0.55 0.60 0.66 RG 0.4 w {x:.2} {bottom:.2} m {x:.2} {top:.2} l S\n"));
        }
    }
    bottom
}

fn draw_table_row(page: &mut Vec<u8>, top: f64, height: f64, row: &TableRow, index: usize) {
    let bottom = top - height;
    let background = if index % 2 == 0 { "0.96 0.97 0.98" } else { "1 1 1" };
    push_ascii(page, &format!("{background} rg {MARGIN:.2} {bottom:.2} {TABLE_WIDTH:.2} {height:.2} re f\n"));
    push_ascii(page, &format!("0.78 0.81 0.84 RG 0.35 w {MARGIN:.2} {bottom:.2} {TABLE_WIDTH:.2} {height:.2} re S\n"));

    let swatch_y = top - 11.5;
    push_ascii(page, &format!("{} {} {} rg\n", row.color.0, row.color.1, row.color.2));
    draw_circle(page, MARGIN + 8.0, swatch_y, 3.0, true);

    let font_size = 7.5;
    let mut x = MARGIN;
    for (cell_index, lines) in row.cells.iter().enumerate() {
        let text_x = if cell_index == 0 { x + 16.0 } else { x + 5.0 };
        for (line_index, line) in lines.iter().enumerate() {
            let baseline = top - 11.0 - line_index as f64 * 9.0;
            draw_text(page, text_x, baseline, font_size,
                Rgb(0.12, 0.16, 0.21), line, line_index == 0 && cell_index == 0);
        }
        x += COLUMN_WIDTHS[cell_index];
        if cell_index < row.cells.len() - 1 {
            push_ascii(page, &format!("0.78 0.81 0.84 RG 0.35 w {x:.2} {bottom:.2} m {x:.2} {top:.2} l S\n"));
        }
    }
}

fn draw_footer(page: &mut Vec<u8>, page_number: usize) {
    push_ascii(page, "0.78 0.81 0.84 RG 0.45 w 36 32 m 559 32 l S\n");
    draw_text(page, MARGIN, 19.0, 7.5, Rgb(0.38, 0.43, 0.49),
        &format!("DPER_Measure · Coordonnées en pixels natifs · Page {page_number}"), false);
    draw_text(page, 387.0, 19.0, 7.5, Rgb(0.38, 0.43, 0.49),
        "Verdict indicatif · U élargie requise", false);
}

fn draw_text(page: &mut Vec<u8>, x: f64, y: f64, size: f64, color: Rgb, text: &str, bold: bool) {
    let font = if bold { "F2" } else { "F1" };
    push_ascii(page, &format!("BT /{font} {size:.2} Tf {:.3} {:.3} {:.3} rg 1 0 0 1 {x:.3} {y:.3} Tm (",
        color.0, color.1, color.2));
    for character in text.chars() {
        if let Some(byte) = win_ansi(character) {
            if matches!(byte, b'(' | b')' | b'\\') { page.push(b'\\'); }
            if byte < 0x20 { page.push(b' '); } else { page.push(byte); }
        } else {
            push_ascii(page, &format!("[U+{:04X}]", character as u32));
        }
    }
    push_ascii(page, ") Tj ET\n");
}

fn pdf_compatible_text(text: &str) -> String {
    let mut output = String::new();
    for character in text.chars() {
        if win_ansi(character).is_some() {
            output.push(character);
        } else {
            output.push_str(&format!("[U+{:04X}]", character as u32));
        }
    }
    output
}

fn win_ansi(character: char) -> Option<u8> {
    let code = character as u32;
    if code <= 0x7f || (0xa0..=0xff).contains(&code) { return Some(code as u8); }
    match character {
        '€' => Some(0x80), '‚' => Some(0x82), 'ƒ' => Some(0x83), '„' => Some(0x84), '…' => Some(0x85),
        '†' => Some(0x86), '‡' => Some(0x87), 'ˆ' => Some(0x88), '‰' => Some(0x89), 'Š' => Some(0x8a),
        '‹' => Some(0x8b), 'Œ' => Some(0x8c), 'Ž' => Some(0x8e), '‘' => Some(0x91), '’' => Some(0x92),
        '“' => Some(0x93), '”' => Some(0x94), '•' => Some(0x95), '–' => Some(0x96), '—' => Some(0x97),
        '˜' => Some(0x98), '™' => Some(0x99), 'š' => Some(0x9a), '›' => Some(0x9b), 'œ' => Some(0x9c),
        'ž' => Some(0x9e), 'Ÿ' => Some(0x9f), _ => None,
    }
}

fn wrap_text(text: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            let word_len = word.chars().count();
            if current.is_empty() {
                current.push_str(word);
            } else if current.chars().count() + 1 + word_len <= max_chars {
                current.push(' ');
                current.push_str(word);
            } else {
                lines.push(std::mem::take(&mut current));
                current.push_str(word);
            }
            while current.chars().count() > max_chars {
                let tail: String = current.chars().skip(max_chars).collect();
                current = current.chars().take(max_chars).collect();
                lines.push(std::mem::take(&mut current));
                current = tail;
            }
        }
        if !current.is_empty() { lines.push(current); }
    }
    if lines.is_empty() { lines.push(String::new()); }
    lines
}

fn limit_lines(mut lines: Vec<String>, max_lines: usize) -> Vec<String> {
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        if let Some(last) = lines.last_mut() { last.push_str("..."); }
    }
    lines
}

fn shorten(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars { return text.to_owned(); }
    let mut shortened: String = text.chars().take(max_chars.saturating_sub(3)).collect();
    shortened.push_str("...");
    shortened
}

fn default_dash(value: &str) -> &str {
    if value.trim().is_empty() { "—" } else { value }
}

fn measurement_color(id: u64) -> Rgb {
    let index = (id.saturating_sub(1) % PALETTE.len() as u64) as usize;
    PALETTE[index]
}

fn utc_timestamp() -> String {
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    utc_timestamp_at(seconds)
}

pub fn utc_timestamp_at(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let day_seconds = seconds % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
    let year_of_era = (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_part + 2) / 5 + 1;
    let month = month_part + if month_part < 10 { 3 } else { -9 };
    if month <= 2 { year += 1; }
    let hour = day_seconds / 3_600;
    let minute = (day_seconds % 3_600) / 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02} UTC")
}

pub fn utc_timestamp_for_export() -> String { utc_timestamp() }

fn stream_object(dictionary: &str, data: &[u8]) -> Vec<u8> {
    let mut object = format!("{dictionary}\nstream\n").into_bytes();
    object.extend_from_slice(data);
    object.extend_from_slice(b"\nendstream");
    object
}

fn push_ascii(output: &mut Vec<u8>, text: &str) {
    output.extend_from_slice(text.as_bytes());
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use std::{fs, path::PathBuf, time::{SystemTime, UNIX_EPOCH}};

    fn measurement(id: u64, kind: Tool, x: f64, y: f64) -> Measurement {
        let points = match kind {
            Tool::Circle => vec![Point { x, y }, Point { x: x + 8.0, y }],
            _ => vec![Point { x, y }, Point { x: x + 8.0, y: y + 3.0 }],
        };
        Measurement {
            id,
            kind,
            is_construction: false,

            points,
            label: format!("M{id}"),
            label_position_native_px: None,
            capture_id: "capture-test".into(),
            calibration_id: None,
            mm_per_pixel: None,
            mm_per_pixel_x: None,
            mm_per_pixel_y: None,
            expanded_uncertainty_mm: None,
            origin: "test".into(),
            method: None,
            protocol_id: None,
            protocol_version: None,
        }
    }

    fn dense_measurements() -> Vec<Measurement> {
        (0..100).map(|index| {
            let (kind, local_index) = if index < 50 {
                (Tool::Distance, index)
            } else {
                (Tool::Circle, index - 50)
            };
            let column = (local_index % 10) as f64;
            let row = (local_index / 10) as f64;
            let type_offset = if kind == Tool::Circle { 500.0 } else { 0.0 };
            measurement(index as u64 + 1, kind,
                type_offset + column * 32.0 + 20.0, row * 36.0 + 20.0)
        }).collect()
    }

    fn report_photo(include_reference: bool) -> FolderReportPhoto {
        let measurements = dense_measurements();
        let reference = include_reference.then(|| FolderReportReference {
            source_name: "plan-test.pdf".into(),
            source_error: None,
            verdict_notice: None,
            overall_status: "Dans la tolérance".into(),
            pages: Vec::new(),
            dimensions: measurements.iter().take(99).map(|measurement| {
                FolderReportReferenceDimension {
                    id: format!("dimension-{}", measurement.id),
                    measurement_id: Some(measurement.id),
                    label: "Mesure".into(),
                    source_text: format!("Mesure {} : 2,000 mm ± 0,100 mm", measurement.id),
                    ocr_page: None,
                    ocr_bbox: None,
                    ocr_confidence: None,
                    nominal_mm: 2.0,
                    tolerance_minus_mm: 0.1,
                    tolerance_plus_mm: 0.1,
                    measured_mm: Some(2.02),
                    expanded_uncertainty_mm: Some(0.01),
                    lower_limit_mm: 1.9,
                    upper_limit_mm: 2.1,
                    status: "Dans la tolérance".into(),
                }
            }).collect(),
        });
        FolderReportPhoto {
            name: "dense-test.png".into(),
            description: "Essai de rapport dense".into(),
            warning: None,
            timestamp: None,
            include_timestamp: false,
            session: Session {
                schema_version: 8,
                automated_inspection: None,
                capture_id: "capture-test".into(),
                image_file: "dense-test.png".into(),
                device: "Caméra de test".into(),
                device_id: "camera-test".into(),
                optical_setup: "configuration de test".into(),
                calibration: None,
                calibrations: Vec::new(),
                measurements,
                annotations: Vec::new(),
                quality_thresholds: Default::default(),
                quality_assessment: None,
                display_correction: Default::default(),
                protocol: None,
                reference_document: None,
                measurement_audit_history: Vec::new(),
            },
            image_rgb: vec![230; 1000 * 1000 * 3],
            width: 1000,
            height: 1000,
            reference,
            series_notice: None,
        }
    }

    pub(crate) fn dense_photo() -> FolderReportPhoto {
        report_photo(true)
    }

    pub(crate) fn historical_vision_photo() -> FolderReportPhoto {
        let mut photo = dense_photo();
        photo.session.measurements.truncate(1);
        photo.session.measurements[0].origin = "vision".into();
        photo.reference.as_mut().unwrap().dimensions.truncate(1);
        let points = photo.session.measurements[0].points.clone();
        let record = crate::dper_inspection::InspectionRecord {
            feature_id: "historique-vision".into(), drawing_binding_id: None, group: "Groupe historique".into(),
            capture_id: photo.session.capture_id.clone(), source_fingerprint: 0,
            calibration_id: None, calibration_fingerprint: None,
            result: crate::dper_vision::InspectionResult {
                geometry: crate::dper_vision::InspectionGeometry::Circle { center: points[0], radius: 8.0 },
                evidence: crate::dper_vision::VisionEvidence {
                    accepted_points: points.clone(), rejected_points: Vec::new(), sample_count: 2,
                    rms_residual_px: 0.0, max_residual_px: 0.0, coverage_fraction: 1.0,
                    endpoint_extent_verified: false,
                },
            },
            value_mm: Some(1.0), outcome: crate::dper_metrology::ValidationOutcome {
                geometry_state: crate::dper_metrology::GeometryState::Validated,
                conformity_status: crate::dper_metrology::ConformityStatus::Conforming,
                blocking_reasons: Vec::new(),
            },
            measurement_id: Some(photo.session.measurements[0].id), measurement_points: points,
            imported: true,
        };
        photo.session.automated_inspection = Some(crate::dper_inspection::InspectionWorkspace {
            records: vec![record], ..Default::default()
        });
        photo
    }

    pub(crate) fn dense_options() -> FolderReportOptions {
        FolderReportOptions {
            title: "Rapport de contrôle".into(),
            include_title: true,
            folder_name: "Lot de test".into(),
            include_folder_name: true,
            include_picture_count: true,
            include_generation_date: false,
            include_sequence_numbers: true,
            include_file_names: true,
            include_descriptions: true,
            include_measurements: true,
            include_audit_page: false,
            operator_name: "Opérateur test".into(),
            part_reference: "MEMS-X13T".into(),
            drawing_revision: "A".into(),
            lot_number: "LOT-TEST".into(),
            audit_notes: String::new(),
            approval_status: "À confirmer".into(),
            approved_by: String::new(),
        }
    }

    pub(crate) fn temporary_path(extension: &str) -> PathBuf {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir())
            .join("target")
            .join(format!("dper_measure_report_test_{}_{stamp}.{extension}", std::process::id()))
    }

    pub(crate) struct RemoveOnDrop(pub(crate) PathBuf);

    impl Drop for RemoveOnDrop {
        fn drop(&mut self) {
            if std::env::var_os("DPER_MEASURE_KEEP_TEST_OUTPUTS").is_none() {
                let _ = fs::remove_file(&self.0);
            }
        }
    }

    #[test]
    fn dense_report_groups_by_type_then_splits_each_type_into_spatial_zones() {
        let photo = report_photo(false);

        let groups = build_report_groups(&photo);

        assert_eq!(groups.len(), 20);
        assert!(groups.iter().all(|group| group.measurement_ids.len() <= MAX_GROUP_MEASUREMENTS));
        assert_eq!(groups.iter().filter(|group| group.description.starts_with("Distance · zone ")).count(), 10);
        assert_eq!(groups.iter().filter(|group| group.description.starts_with("Cercle (centre + bord) · zone ")).count(), 10);
        let mut grouped_ids = groups.into_iter().flat_map(|group| group.measurement_ids).collect::<Vec<_>>();
        grouped_ids.sort_unstable();
        assert_eq!(grouped_ids, (1..=100).collect::<Vec<_>>());
    }

    #[test]
    fn dense_pdf_writes_numbered_group_views_tables_and_reference_statuses() {
        let photo = dense_photo();
        let options = dense_options();
        let path = temporary_path("pdf");
        let _cleanup = RemoveOnDrop(path.clone());

        write_folder_pdf(&path, &options, &[photo]).expect("PDF de contrôle dense");
        if std::env::var_os("DPER_MEASURE_KEEP_TEST_OUTPUTS").is_some() {
            println!("PDF_FIXTURE_PATH={}", path.display());
        }
        let bytes = fs::read(&path).expect("lecture du PDF généré");
        assert!(bytes.starts_with(b"%PDF-"));
        for marker in [b"Groupe 1".as_slice(), b"Groupe 20", b"M100", b"Mesure / cote", b"Nominal", b"Dans la tol", b"Non associ"] {
            assert!(bytes.windows(marker.len()).any(|window| window == marker),
                "Marqueur PDF absent : {}", String::from_utf8_lossy(marker));
        }
        let page_marker = b"/Type /Page";
        assert!(bytes.windows(page_marker.len()).any(|window| window == page_marker));
    }

    #[test]
    fn pdf_suspends_vision_and_always_includes_historical_evidence_notice() {
        let photo = crate::report::test_support::historical_vision_photo();
        let options = dense_options();
        assert!(!options.include_audit_page);
        let path = temporary_path("pdf");
        let _cleanup = RemoveOnDrop(path.clone());
        write_folder_pdf(&path, &options, &[photo]).unwrap();
        let bytes = fs::read(path).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        for marker in ["Mesures vision", "Historique non", "historique-vision", "Verdict suspendu"] {
            assert!(text.contains(marker), "Notice PDF absente : {marker}");
        }
        assert!(!text.contains("Dans la tol"), "Un état historique ne doit pas accorder un verdict courant");
    }
}
