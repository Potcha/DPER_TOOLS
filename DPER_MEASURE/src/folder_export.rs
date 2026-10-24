//! Sélection et métadonnées des images pour l'export d'un dossier.
use crate::{reference::{self, ReferenceDocument}, report::{FolderReportPhoto, FolderReportReference, FolderReportReferenceDimension}, storage::{self, Session}};
use eframe::egui;
use image::GenericImageView;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::{Path, PathBuf}, time::{SystemTime, UNIX_EPOCH}};

const PREFERENCES_FILE: &str = ".dper_measure_report.json";

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub title: String,
    pub include_title: bool,
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
    pub selected_template: String,
    pub template_name_draft: String,
    pub templates: Vec<ReportTemplate>,
    descriptions: HashMap<String, String>,
    timestamps: HashMap<String, bool>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ReportTemplate {
    pub name: String,
    pub title: String,
    pub include_title: bool,
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

impl Default for Preferences {
    fn default() -> Self {
        Self {
            title: String::new(), include_title: true, include_folder_name: true,
            include_picture_count: true, include_generation_date: true,
            include_sequence_numbers: true, include_file_names: true,
            include_descriptions: true, include_measurements: true,
            include_audit_page: true, operator_name: String::new(), part_reference: String::new(),
            drawing_revision: String::new(), lot_number: String::new(), audit_notes: String::new(),
            approval_status: "À renseigner".into(), approved_by: String::new(),
            selected_template: String::new(), template_name_draft: String::new(), templates: Vec::new(),
            descriptions: HashMap::new(), timestamps: HashMap::new(),
        }
    }
}

impl Preferences {
    pub fn save_template(&mut self) -> Result<(), String> {
        let name = self.template_name_draft.trim().to_owned();
        if name.is_empty() || name.chars().count() > 80 {
            return Err("Saisissez un nom de modèle de rapport (1 à 80 caractères).".into());
        }
        let template = ReportTemplate {
            name: name.clone(), title: self.title.clone(),
            include_title: self.include_title, include_folder_name: self.include_folder_name,
            include_picture_count: self.include_picture_count, include_generation_date: self.include_generation_date,
            include_sequence_numbers: self.include_sequence_numbers, include_file_names: self.include_file_names,
            include_descriptions: self.include_descriptions, include_measurements: self.include_measurements,
            include_audit_page: self.include_audit_page, operator_name: self.operator_name.clone(),
            part_reference: self.part_reference.clone(), drawing_revision: self.drawing_revision.clone(),
            lot_number: self.lot_number.clone(), audit_notes: self.audit_notes.clone(),
            approval_status: self.approval_status.clone(), approved_by: self.approved_by.clone(),
        };
        if let Some(existing) = self.templates.iter_mut().find(|template| template.name.eq_ignore_ascii_case(&name)) {
            *existing = template;
        } else {
            self.templates.push(template);
            self.templates.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
        }
        self.selected_template = name.clone();
        self.template_name_draft = name;
        Ok(())
    }

    pub fn apply_template(&mut self, name: &str) -> bool {
        let Some(template) = self.templates.iter().find(|template| template.name == name).cloned() else { return false; };
        self.title = template.title; self.include_title = template.include_title;
        self.include_folder_name = template.include_folder_name;
        self.include_picture_count = template.include_picture_count;
        self.include_generation_date = template.include_generation_date;
        self.include_sequence_numbers = template.include_sequence_numbers;
        self.include_file_names = template.include_file_names;
        self.include_descriptions = template.include_descriptions;
        self.include_measurements = template.include_measurements;
        self.include_audit_page = template.include_audit_page;
        self.operator_name = template.operator_name; self.part_reference = template.part_reference;
        self.drawing_revision = template.drawing_revision; self.lot_number = template.lot_number;
        self.audit_notes = template.audit_notes; self.approval_status = template.approval_status;
        self.approved_by = template.approved_by;
        self.selected_template = template.name.clone(); self.template_name_draft = template.name;
        true
    }

    pub fn delete_selected_template(&mut self) -> bool {
        if self.selected_template.is_empty() { return false; }
        let selected = self.selected_template.clone();
        let previous_length = self.templates.len();
        self.templates.retain(|template| template.name != selected);
        let removed = self.templates.len() != previous_length;
        if removed { self.selected_template.clear(); self.template_name_draft.clear(); }
        removed
    }
}

pub struct CurrentImage {
    pub source: String,
    pub capture_id: String,
    pub device: String,
    pub device_id: String,
    pub optical_setup: String,
    pub calibration: Option<storage::Calibration>,
    pub calibrations: Vec<storage::Calibration>,
    pub measurements: Vec<crate::geometry::Measurement>,
    pub annotations: Vec<crate::geometry::Annotation>,
    pub quality_thresholds: crate::quality::ImageQualityThresholds,
    pub quality_assessment: Option<crate::quality::ImageQualityAssessment>,
    pub display_correction: crate::quality::DisplayCorrection,
    pub protocol: Option<storage::MeasurementProtocol>,
    pub reference_document: Option<ReferenceDocument>,
    pub measurement_audit_history: Vec<storage::MeasurementAuditEvent>,
    pub rgb: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl CurrentImage {
    pub fn session(&self) -> Session {
        Session {
            schema_version: storage::CURRENT_SCHEMA_VERSION,
            capture_id: self.capture_id.clone(),
            image_file: self.source.clone(),
            device: self.device.clone(),
            device_id: self.device_id.clone(),
            optical_setup: self.optical_setup.clone(),
            calibration: self.calibration.clone(),
            calibrations: self.calibrations.clone(),
            measurements: self.measurements.clone(),
            annotations: self.annotations.clone(),
            quality_thresholds: self.quality_thresholds.clone(),
            quality_assessment: self.quality_assessment.clone(),
            display_correction: self.display_correction,
            protocol: self.protocol.clone(),
            reference_document: self.reference_document.clone(),
            measurement_audit_history: self.measurement_audit_history.clone(),
        }
    }
}

pub struct PhotoRow {
    pub key: String,
    pub name: String,
    pub description: String,
    pub timestamp: Option<String>,
    pub include_timestamp: bool,
    pub selected: bool,
    pub path: Option<PathBuf>,
    pub current_rgb: Option<Vec<u8>>,
    pub width: u32,
    pub height: u32,
    pub session: Session,
    pub thumbnail: Option<egui::TextureHandle>,
    pub error: Option<String>,
    pub warning: Option<String>,
}

pub struct Dialog {
    pub folder: PathBuf,
    pub preferences: Preferences,
    pub rows: Vec<PhotoRow>,
}

impl Dialog {
    pub fn new(ctx: &egui::Context, folder: PathBuf, current: Option<CurrentImage>) -> Self {
        let mut dialog = Self { folder, preferences: Preferences::default(), rows: Vec::new() };
        dialog.load_preferences();
        dialog.scan(ctx, current);
        dialog
    }

    pub fn choose_folder(&mut self, ctx: &egui::Context, current: Option<CurrentImage>) {
        let Some(folder) = rfd::FileDialog::new().set_directory(&self.folder).pick_folder() else { return; };
        self.folder = folder;
        self.load_preferences();
        self.scan(ctx, current);
    }

    pub fn refresh(&mut self, ctx: &egui::Context, current: Option<CurrentImage>) {
        self.load_preferences();
        self.scan(ctx, current);
    }

    pub fn select_all(&mut self, selected: bool) {
        for row in &mut self.rows {
            if row.error.is_none() && !row.session.measurements.is_empty() { row.selected = selected; }
        }
    }

    pub fn selected_count(&self) -> usize {
        self.rows.iter().filter(|row| row.selected && row.error.is_none()
            && !row.session.measurements.is_empty()).count()
    }

    pub fn report_photos(&self) -> Result<Vec<FolderReportPhoto>, String> {
        let selected: Vec<_> = self.rows.iter()
            .filter(|row| row.selected && row.error.is_none() && !row.session.measurements.is_empty()).collect();
        if selected.is_empty() { return Err("Aucune image avec mesure n'est sélectionnée pour le rapport.".into()); }

        let mut photos = Vec::with_capacity(selected.len());
        let series_notice = report_series_notice(&selected);
        for row in selected {
            let (rgb, width, height) = if let Some(rgb) = &row.current_rgb {
                (rgb.clone(), row.width, row.height)
            } else if let Some(path) = &row.path {
                let image = image::open(path)
                    .map_err(|error| format!("Image illisible ({}): {error}", path.display()))?
                    .to_rgb8();
                let (width, height) = image.dimensions();
                (image.into_raw(), width, height)
            } else {
                return Err(format!("Source absente pour {}", row.name));
            };
            let mut session = row.session.clone();
            session.quality_assessment = crate::quality::assess_image_quality_with_correction(
                &rgb, width, height, &session.quality_thresholds, session.display_correction,
            ).ok();
            let quality_notice = match session.quality_assessment.as_ref() {
                Some(assessment) if assessment.is_exploitable => None,
                Some(assessment) => {
                    let issues = assessment.issues.iter().map(|issue| issue.label())
                        .collect::<Vec<_>>().join(", ");
                    Some(if issues.is_empty() {
                        "Verdict suspendu : image non exploitable selon les seuils provisoires.".to_owned()
                    } else {
                        format!("Verdict suspendu : image non exploitable ({issues}).")
                    })
                }
                None => Some("Verdict suspendu : qualité image non évaluée.".to_owned()),
            };
            let protocol_notice = session_protocol_notice(&session);
            let verdict_notice = [quality_notice.as_deref(), protocol_notice.as_deref(), series_notice.as_deref()]
                .into_iter().flatten().collect::<Vec<_>>().join(" ");
            let verdict_notice = (!verdict_notice.is_empty()).then_some(verdict_notice);
            photos.push(FolderReportPhoto {
                name: row.name.clone(), description: row.description.clone(),
                warning: row.warning.clone(),
                timestamp: row.timestamp.clone(), include_timestamp: row.include_timestamp,
                session: session.clone(), image_rgb: rgb, width, height,
                series_notice: series_notice.clone(),
                reference: session.reference_document.as_ref().map(|document| {
                    let base = row.path.as_ref().and_then(|path| path.parent())
                        .unwrap_or(self.folder.as_path());
                    let source = reference::resolve_source(&document.source_file, base);
                    let (pages, source_error) = if source.is_file() {
                        match reference::render_document(&source) {
                            Ok(pages) => (pages, None),
                            Err(error) => (Vec::new(), Some(error)),
                        }
                    } else {
                        (Vec::new(), Some("Fichier source introuvable.".to_owned()))
                    };
                    let dimensions = document.dimensions.iter().map(|dimension| {
                        let comparison = reference::compare_dimension(dimension, &session.measurements);
                        FolderReportReferenceDimension {
                            id: dimension.id.clone(),
                            measurement_id: dimension.measurement_id,
                            label: dimension.label.clone(), source_text: dimension.source_text.clone(),
                            ocr_page: dimension.ocr_page,
                            ocr_bbox: dimension.ocr_bbox.clone(),
                            ocr_confidence: dimension.ocr_confidence,
                            nominal_mm: dimension.nominal_mm,
                            tolerance_minus_mm: dimension.tolerance_minus_mm,
                            tolerance_plus_mm: dimension.tolerance_plus_mm,
                            measured_mm: comparison.measured_mm,
                            lower_limit_mm: comparison.lower_limit_mm,
                            upper_limit_mm: comparison.upper_limit_mm,
                            expanded_uncertainty_mm: comparison.expanded_uncertainty_mm,
                            status: if verdict_notice.is_some() {
                                "Verdict suspendu".to_owned()
                            } else {
                                comparison.status.label().to_owned()
                            },
                        }
                    }).collect();
                    FolderReportReference {
                        source_name: document.source_name.clone(), source_error,
                        verdict_notice: verdict_notice.clone(),
                        overall_status: if verdict_notice.is_some() {
                            "Verdict suspendu · qualité ou comparabilité série".to_owned()
                        } else {
                            reference::part_status(document, &session.measurements).label().to_owned()
                        },
                        pages, dimensions,
                    }
                }),
            });
        }
        Ok(photos)
    }

    pub fn measurements_csv_batch(&self) -> Result<Vec<u8>, String> {
        let selected = self.rows.iter().filter(|row| row.selected && row.error.is_none()
            && !row.session.measurements.is_empty()).collect::<Vec<_>>();
        if selected.is_empty() {
            return Err("Sélectionnez au moins une image avec mesure pour le CSV par lot.".into());
        }
        let series_notice = report_series_notice(&selected);
        let mut output = String::from(
            "image;capture_id;appareil_id;configuration_optique;protocole_id;protocole_version;mesure_id;libelle;type;resultat;nominal_mm;tol_moins_mm;tol_plus_mm;mesure_mm;ecart_mm;u_mm;statut;motif_suspension;reference_ocr_page;reference_ocr_confiance_pct\r\n",
        );
        for row in selected {
            let quality_notice = batch_quality_notice(row);
            let protocol_notice = session_protocol_notice(&row.session);
            let notice = [quality_notice.as_deref(), protocol_notice.as_deref(), series_notice.as_deref()]
                .into_iter().flatten().collect::<Vec<_>>().join(" ");
            let notice = (!notice.is_empty()).then_some(notice);
            let reference = row.session.reference_document.as_ref();
            let mut written_dimensions = Vec::<String>::new();
            for measurement in &row.session.measurements {
                let associated = reference.map(|document| document.dimensions.iter()
                    .filter(|dimension| dimension.measurement_id == Some(measurement.id))
                    .collect::<Vec<_>>()).unwrap_or_default();
                if associated.is_empty() {
                    write_batch_csv_row(&mut output, row, measurement, None, notice.as_deref());
                } else {
                    for dimension in associated {
                        written_dimensions.push(dimension.id.clone());
                        write_batch_csv_row(&mut output, row, measurement, Some(dimension), notice.as_deref());
                    }
                }
            }
            if let Some(reference) = reference {
                for dimension in &reference.dimensions {
                    if written_dimensions.contains(&dimension.id) { continue; }
                    let comparison = reference::compare_dimension(dimension, &row.session.measurements);
                    let status = notice.as_ref().map(|_| "Verdict suspendu".to_owned())
                        .unwrap_or_else(|| comparison.status.label().to_owned());
                    append_batch_csv_fields(&mut output, [
                        row.name.clone(), row.session.capture_id.clone(), row.session.device_id.clone(),
                        row.session.optical_setup.clone(), row.session.protocol.as_ref().map(|protocol| protocol.id.clone()).unwrap_or_default(),
                        row.session.protocol.as_ref().map(|protocol| protocol.version.to_string()).unwrap_or_default(),
                        String::new(), dimension.label.clone(), String::new(), String::new(),
                        format!("{:.8}", dimension.nominal_mm),
                        format!("{:.8}", dimension.tolerance_minus_mm),
                        format!("{:.8}", dimension.tolerance_plus_mm),
                        comparison.measured_mm.map(|value| format!("{value:.8}")).unwrap_or_default(),
                        comparison.measured_mm.map(|value| format!("{:+.8}", value - dimension.nominal_mm)).unwrap_or_default(),
                        comparison.expanded_uncertainty_mm.map(|value| format!("{value:.8}")).unwrap_or_default(),
                        status, notice.clone().unwrap_or_default(),
                        dimension.ocr_page.map(|page| page.to_string()).unwrap_or_default(),
                        dimension.ocr_confidence.map(|value| format!("{value:.1}")).unwrap_or_default(),
                    ]);
                }
            }
        }
        Ok(output.into_bytes())
    }

    pub fn measurements_xlsx_batch(&self) -> Result<Vec<u8>, String> {
        let selected = self.rows.iter().filter(|row| row.selected && row.error.is_none()
            && !row.session.measurements.is_empty()).collect::<Vec<_>>();
        if selected.is_empty() {
            return Err("Sélectionnez au moins une image avec mesure pour le classeur XLSX du lot.".into());
        }
        let series_notice = report_series_notice(&selected);
        let rows = selected.iter().map(|row| {
            let quality_notice = batch_quality_notice(row);
            let protocol_notice = session_protocol_notice(&row.session);
            let notice = [quality_notice.as_deref(), protocol_notice.as_deref(), series_notice.as_deref()]
                .into_iter().flatten().collect::<Vec<_>>().join(" ");
            (row.name.as_str(), &row.session, (!notice.is_empty()).then_some(notice))
        }).collect::<Vec<_>>();
        crate::exports::measurements_xlsx_batch(&rows)
    }

    pub fn report_preferences(&self) -> Preferences {
        let mut preferences = self.preferences.clone();
        preferences.descriptions.clear();
        preferences.timestamps.clear();
        for row in &self.rows {
            preferences.descriptions.insert(row.key.clone(), row.description.clone());
            preferences.timestamps.insert(row.key.clone(), row.include_timestamp);
        }
        preferences
    }

    pub fn save_template(&mut self) -> Result<(), String> {
        let previous = self.preferences.clone();
        self.preferences.save_template()?;
        if let Err(error) = self.save_preferences() {
            self.preferences = previous;
            return Err(format!("Le modèle n'a pas été enregistré dans le dossier : {error}"));
        }
        Ok(())
    }

    pub fn delete_selected_template(&mut self) -> Result<bool, String> {
        let previous = self.preferences.clone();
        if !self.preferences.delete_selected_template() {
            return Ok(false);
        }
        if let Err(error) = self.save_preferences() {
            self.preferences = previous;
            return Err(format!("La suppression du modèle n'a pas été enregistrée dans le dossier : {error}"));
        }
        Ok(true)
    }

    pub fn save_preferences(&self) -> Result<(), String> {
        let path = self.folder.join(PREFERENCES_FILE);
        let body = serde_json::to_vec_pretty(&self.report_preferences())
            .map_err(|error| error.to_string())?;
        fs::write(path, body).map_err(|error| error.to_string())
    }

    fn load_preferences(&mut self) {
        self.preferences = fs::read(self.folder.join(PREFERENCES_FILE))
            .ok().and_then(|body| serde_json::from_slice(&body).ok())
            .unwrap_or_default();
    }

    fn scan(&mut self, ctx: &egui::Context, current: Option<CurrentImage>) {
        self.rows.clear();
        let mut paths = match fs::read_dir(&self.folder) {
            Ok(entries) => entries.filter_map(Result::ok).map(|entry| entry.path())
                .filter(|path| is_supported_image(path)).collect::<Vec<_>>(),
            Err(_) => Vec::new(),
        };
        paths.sort_by(|left, right| {
            let left_modified = fs::metadata(left).and_then(|meta| meta.modified()).unwrap_or(UNIX_EPOCH);
            let right_modified = fs::metadata(right).and_then(|meta| meta.modified()).unwrap_or(UNIX_EPOCH);
            left_modified.cmp(&right_modified).then_with(|| left.file_name().cmp(&right.file_name()))
        });

        let sessions = load_sessions(&self.folder);
        let current_path = current.as_ref().and_then(|image| canonical(&PathBuf::from(&image.source)));
        let mut current_matched = false;
        for path in paths {
            let key = path.file_name().and_then(|name| name.to_str())
                .unwrap_or("image").to_owned();
            let metadata = fs::metadata(&path).ok();
            let timestamp = metadata.as_ref().and_then(|meta| meta.modified().ok()).map(format_timestamp);
            let image = image::open(&path);
            let (width, height, thumbnail, error) = match image {
                Ok(image) => {
                    let thumbnail_image = image.thumbnail(112, 72).to_rgb8();
                    let (tw, th) = thumbnail_image.dimensions();
                    let color = egui::ColorImage::from_rgb([tw as usize, th as usize], thumbnail_image.as_raw());
                    let texture = ctx.load_texture(format!("export-thumb-{key}"), color, egui::TextureOptions::LINEAR);
                    let (width, height) = image.dimensions();
                    (width, height, Some(texture), None)
                }
                Err(error) => (0, 0, None, Some(error.to_string())),
            };
            let path_key = canonical(&path);
            let is_current = current_path.is_some() && path_key == current_path;
            if is_current { current_matched = true; }
            let mut session = if is_current {
                current.as_ref().expect("current path matched").session()
            } else {
                sessions.iter().find(|(session_path, _)| Some(session_path) == path_key.as_ref())
                    .map(|(_, session)| session.clone())
                    .unwrap_or_else(|| empty_session(key.clone(), path.to_string_lossy().into_owned()))
            };
            let mut warning = None;
            if !is_current && !session_is_valid(&session, width, height) {
                session = empty_session(key.clone(), path.to_string_lossy().into_owned());
                warning = Some("Mesures du projet associées ignorées : données incompatibles.".to_owned());
            }
            let (width, height, thumbnail, current_rgb) = if is_current {
                let current = current.as_ref().expect("current path matched");
                (current.width, current.height,
                    thumbnail_from_rgb(ctx, &current.rgb, current.width, current.height, &key),
                    Some(current.rgb.clone()))
            } else { (width, height, thumbnail, None) };
            self.rows.push(PhotoRow {
                description: self.preferences.descriptions.get(&key).cloned().unwrap_or_default(),
                include_timestamp: self.preferences.timestamps.get(&key).copied().unwrap_or(true),
                key: key.clone(), name: key, timestamp,
                selected: error.is_none() && !session.measurements.is_empty(), path: Some(path),
                current_rgb, width, height, session, thumbnail, error, warning,
            });
        }

        if let Some(current) = current.filter(|_| !current_matched) {
            let source_path = PathBuf::from(&current.source);
            let display_name = source_path.file_name().and_then(|name| name.to_str())
                .filter(|name| !name.is_empty()).map(str::to_owned)
                .unwrap_or_else(|| format!("Capture {}", current.capture_id));
            let key = format!("current:{}", current.capture_id);
            let thumbnail = thumbnail_from_rgb(ctx, &current.rgb, current.width, current.height, &key);
            let timestamp = Some(format_timestamp(SystemTime::now()));
            let session = current.session();
            self.rows.insert(0, PhotoRow {
                description: self.preferences.descriptions.get(&key).cloned().unwrap_or_default(),
                include_timestamp: self.preferences.timestamps.get(&key).copied().unwrap_or(true),
                key: key.clone(), name: display_name, timestamp,
                selected: !session.measurements.is_empty(),
                path: None, current_rgb: Some(current.rgb), width: current.width, height: current.height,
                session, thumbnail, error: None, warning: None,
            });
        }
    }
}

fn batch_quality_notice(row: &PhotoRow) -> Option<String> {
    let loaded = if let Some(rgb) = row.current_rgb.as_ref() {
        Some((rgb.clone(), row.width, row.height))
    } else if let Some(path) = row.path.as_ref() {
        image::open(path).ok().map(|image| {
            let image = image.to_rgb8();
            let (width, height) = image.dimensions();
            (image.into_raw(), width, height)
        })
    } else { None };
    let assessment = loaded.and_then(|(rgb, width, height)| crate::quality::assess_image_quality_with_correction(
        &rgb, width, height, &row.session.quality_thresholds, row.session.display_correction,
    ).ok());
    match assessment {
        Some(assessment) if assessment.is_exploitable => None,
        Some(assessment) => {
            let issues = assessment.issues.iter().map(|issue| issue.label()).collect::<Vec<_>>().join(", ");
            Some(if issues.is_empty() {
                "Image non exploitable selon les seuils provisoires".to_owned()
            } else {
                format!("Image non exploitable : {issues}.")
            })
        }
        None => Some("Qualité image non évaluée".to_owned()),
    }
}

fn write_batch_csv_row(
    output: &mut String,
    row: &PhotoRow,
    measurement: &crate::geometry::Measurement,
    dimension: Option<&reference::ReferenceDimension>,
    notice: Option<&str>,
) {
    let comparison = dimension.map(|dimension| reference::compare_dimension(dimension, &row.session.measurements));
    let measured_mm = comparison.and_then(|comparison| comparison.measured_mm)
        .or_else(|| reference::measurement_mm(measurement));
    let deviation = dimension.and_then(|dimension| measured_mm.map(|value| value - dimension.nominal_mm));
    let status = if notice.is_some() { "Verdict suspendu".to_owned() }
        else if let Some(comparison) = comparison { comparison.status.label().to_owned() }
        else if row.session.reference_document.is_some() { "Non associée".to_owned() }
        else { "Pas de cote de référence".to_owned() };
    append_batch_csv_fields(output, [
        row.name.clone(), row.session.capture_id.clone(), row.session.device_id.clone(),
        row.session.optical_setup.clone(),
        row.session.protocol.as_ref().map(|protocol| protocol.id.clone()).unwrap_or_default(),
        row.session.protocol.as_ref().map(|protocol| protocol.version.to_string()).unwrap_or_default(),
        measurement.id.to_string(), measurement.label.clone(), measurement.kind.label().to_owned(),
        crate::geometry::result(measurement),
        dimension.map(|dimension| format!("{:.8}", dimension.nominal_mm)).unwrap_or_default(),
        dimension.map(|dimension| format!("{:.8}", dimension.tolerance_minus_mm)).unwrap_or_default(),
        dimension.map(|dimension| format!("{:.8}", dimension.tolerance_plus_mm)).unwrap_or_default(),
        measured_mm.map(|value| format!("{value:.8}")).unwrap_or_default(),
        deviation.map(|value| format!("{value:+.8}")).unwrap_or_default(),
        comparison.and_then(|comparison| comparison.expanded_uncertainty_mm)
            .or(measurement.expanded_uncertainty_mm).map(|value| format!("{value:.8}")).unwrap_or_default(),
        status, notice.unwrap_or_default().to_owned(),
        dimension.and_then(|dimension| dimension.ocr_page).map(|page| page.to_string()).unwrap_or_default(),
        dimension.and_then(|dimension| dimension.ocr_confidence).map(|value| format!("{value:.1}")).unwrap_or_default(),
    ]);
}

fn append_batch_csv_fields<const N: usize>(output: &mut String, fields: [String; N]) {
    for (index, field) in fields.iter().enumerate() {
        if index > 0 { output.push(';'); }
        if field.chars().any(|character| matches!(character, ';' | '"' | '\r' | '\n')) {
            output.push('"');
            output.push_str(&field.replace('"', "\"\""));
            output.push('"');
        } else {
            output.push_str(field);
        }
    }
    output.push_str("\r\n");
}

pub(crate) fn session_protocol_notice(session: &Session) -> Option<String> {
    if session.schema_version < 8 { return None; }
    let Some(protocol) = session.protocol.as_ref() else {
        return session.measurements.iter().any(|measurement|
            measurement.protocol_id.is_some() || measurement.protocol_version.is_some())
            .then(|| "Verdict suspendu : les mesures portent un protocole qui n'est pas actif dans cette session.".to_owned());
    };
    let mut reasons = Vec::new();
    if session.measurements.iter().any(|measurement|
        measurement.protocol_id.as_deref() != Some(protocol.id.as_str())
            || measurement.protocol_version != Some(protocol.version))
    {
        reasons.push("mesures non associées à la version active");
    }
    if (!protocol.device.is_empty() && protocol.device != session.device)
        || (!protocol.device_id.is_empty() && protocol.device_id != session.device_id)
        || protocol.optical_setup != session.optical_setup
    {
        reasons.push("caméra ou configuration optique différente");
    }
    if session.quality_assessment.as_ref().is_some_and(|assessment|
        assessment.image_dimensions != protocol.resolution)
    {
        reasons.push("résolution différente");
    }
    let calibration_matches = match session.calibration.as_ref() {
        Some(profile) => protocol.calibration_profile_id.as_deref() == Some(profile.id.as_str())
            && protocol.device_id == profile.device_id
            && protocol.mm_per_pixel.is_some_and(|scale|
                scale.to_bits() == profile.mm_per_pixel.to_bits())
            && protocol.scale_x_mm_per_pixel.map(f64::to_bits)
                == profile.scale_x_mm_per_pixel.map(f64::to_bits)
            && protocol.scale_y_mm_per_pixel.map(f64::to_bits)
                == profile.scale_y_mm_per_pixel.map(f64::to_bits)
            && protocol.expanded_uncertainty_mm.map(f64::to_bits)
                == profile.expanded_uncertainty_mm.map(f64::to_bits),
        None => protocol.calibration_profile_id.is_none()
            && protocol.mm_per_pixel.is_none()
            && protocol.scale_x_mm_per_pixel.is_none()
            && protocol.scale_y_mm_per_pixel.is_none()
            && protocol.expanded_uncertainty_mm.is_none(),
    };
    if !calibration_matches {
        reasons.push("profil de calibration ou incertitude différente");
    }
    let thresholds_signature = |thresholds: &crate::quality::ImageQualityThresholds| [
        thresholds.min_mean_luminance.to_bits(),
        thresholds.max_mean_luminance.to_bits(),
        thresholds.min_contrast_std_dev.to_bits(),
        thresholds.min_laplacian_variance.to_bits(),
        thresholds.min_edge_visibility_fraction.to_bits(),
        thresholds.max_clipped_pixel_fraction.to_bits(),
        thresholds.edge_gradient_threshold.to_bits(),
    ];
    if thresholds_signature(&session.quality_thresholds) != thresholds_signature(&protocol.quality_thresholds) {
        reasons.push("seuils de qualité différents");
    }
    let correction_signature = |correction: crate::quality::DisplayCorrection|
        (correction.brightness.to_bits(), correction.contrast.to_bits(), correction.gamma.to_bits());
    if correction_signature(session.display_correction) != correction_signature(protocol.display_correction) {
        reasons.push("correction d'affichage différente");
    }
    if protocol.expected_measurements.is_empty()
        || protocol.expected_measurements.iter().any(|characteristic|
            characteristic.reference.as_ref().is_some_and(|dimension| !dimension.confirmed))
        || protocol_characteristic_signature(session)
            != protocol_characteristic_signature_from_protocol(&protocol.expected_measurements)
    {
        reasons.push("caractéristiques ou tolérances différentes du gabarit");
    }
    if reasons.is_empty() { None }
    else { Some(format!("Verdict suspendu : {}.", reasons.join("; "))) }
}

fn report_series_notice(rows: &[&PhotoRow]) -> Option<String> {
    if rows.len() < 2 { return None; }
    let protocols = rows.iter().filter_map(|row| row.session.protocol.as_ref()).collect::<Vec<_>>();
    if protocols.len() != rows.len() {
        return Some("Verdict de série suspendu : chaque image doit utiliser un protocole versionné commun.".into());
    }
    let baseline_protocol = protocols[0];
    let mut reasons = Vec::new();
    if protocols.iter().any(|protocol| protocol.id != baseline_protocol.id
        || protocol.version != baseline_protocol.version
        || protocol.part_family != baseline_protocol.part_family
        || protocol.device != baseline_protocol.device
        || protocol.device_id != baseline_protocol.device_id
        || protocol.optical_setup != baseline_protocol.optical_setup
        || protocol.resolution != baseline_protocol.resolution
        || protocol.display_precision_mm != baseline_protocol.display_precision_mm
        || protocol.tolerance_rule != baseline_protocol.tolerance_rule)
    {
        reasons.push("versions ou règles de protocole différentes");
    }
    if rows.iter().any(|row| row.session.device != rows[0].session.device
        || row.session.device_id != rows[0].session.device_id
        || row.session.optical_setup != rows[0].session.optical_setup
        || row.width != rows[0].width || row.height != rows[0].height)
    {
        reasons.push("caméra, configuration optique ou résolution différente");
    }
    let calibration_signature = |row: &PhotoRow| {
        let profile = row.session.calibration.as_ref();
        let protocol = row.session.protocol.as_ref();
        (
            profile.map(|profile| profile.id.clone())
                .or_else(|| protocol.and_then(|protocol| protocol.calibration_profile_id.clone())),
            profile.map(|profile| profile.mm_per_pixel.to_bits())
                .or_else(|| protocol.and_then(|protocol| protocol.mm_per_pixel.map(f64::to_bits))),
            profile.and_then(|profile| profile.scale_x_mm_per_pixel.map(f64::to_bits))
                .or_else(|| protocol.and_then(|protocol| protocol.scale_x_mm_per_pixel.map(f64::to_bits))),
            profile.and_then(|profile| profile.scale_y_mm_per_pixel.map(f64::to_bits))
                .or_else(|| protocol.and_then(|protocol| protocol.scale_y_mm_per_pixel.map(f64::to_bits))),
            profile.and_then(|profile| profile.expanded_uncertainty_mm)
                .map(f64::to_bits)
                .or_else(|| protocol.and_then(|protocol| protocol.expanded_uncertainty_mm.map(f64::to_bits))),
        )
    };
    if rows.iter().any(|row| calibration_signature(row) != calibration_signature(rows[0])) {
        reasons.push("profil d'étalonnage ou incertitude différente");
    }
    let thresholds_signature = |thresholds: &crate::quality::ImageQualityThresholds| [
        thresholds.min_mean_luminance.to_bits(),
        thresholds.max_mean_luminance.to_bits(),
        thresholds.min_contrast_std_dev.to_bits(),
        thresholds.min_laplacian_variance.to_bits(),
        thresholds.min_edge_visibility_fraction.to_bits(),
        thresholds.max_clipped_pixel_fraction.to_bits(),
        thresholds.edge_gradient_threshold.to_bits(),
    ];
    if rows.iter().any(|row| thresholds_signature(&row.session.quality_thresholds)
        != thresholds_signature(&rows[0].session.quality_thresholds))
    {
        reasons.push("seuils de qualité d'image différents");
    }
    let correction_signature = |correction: crate::quality::DisplayCorrection|
        (correction.brightness.to_bits(), correction.contrast.to_bits(), correction.gamma.to_bits());
    if rows.iter().any(|row| correction_signature(row.session.display_correction)
        != correction_signature(rows[0].session.display_correction))
    {
        reasons.push("corrections d'affichage différentes");
    }
    let actual_characteristics = |row: &PhotoRow| protocol_characteristic_signature(&row.session);
    if rows.iter().any(|row| actual_characteristics(row) != actual_characteristics(rows[0])) {
        reasons.push("types, caractéristiques ou méthodes de mesure différents");
    }
    if rows.iter().any(|row| {
        let protocol = row.session.protocol.as_ref().unwrap();
        (!protocol.device.is_empty() && protocol.device != row.session.device)
            || protocol.expected_measurements.is_empty()
            || protocol.expected_measurements.iter().any(|characteristic|
                characteristic.reference.as_ref().is_some_and(|reference| !reference.confirmed))
            || protocol_characteristic_signature_from_protocol(&protocol.expected_measurements)
                != actual_characteristics(row)
            || (row.session.schema_version >= 8 && row.session.measurements.iter().any(|measurement|
                measurement.protocol_id.as_deref() != Some(protocol.id.as_str())
                    || measurement.protocol_version != Some(protocol.version)))
    }) {
        reasons.push("mesures ou tolérances différentes du gabarit versionné");
    }
    let reference_signature = |row: &PhotoRow| row.session.reference_document.as_ref().map(|document| {
        let dimensions = document.dimensions.iter().map(|dimension| format!(
            "{}:{:016x}:{:016x}:{:016x}:{}",
            dimension.label,
            dimension.nominal_mm.to_bits(),
            dimension.tolerance_minus_mm.to_bits(),
            dimension.tolerance_plus_mm.to_bits(),
            dimension.confirmed,
        )).collect::<Vec<_>>().join("|");
        format!("{}|{dimensions}", document.source_name)
    });
    if rows.iter().any(|row| reference_signature(row) != reference_signature(rows[0])) {
        reasons.push("documents de référence ou cotes différentes");
    }
    if reasons.is_empty() { None }
    else { Some(format!("Verdict de série suspendu : {}.", reasons.join("; "))) }
}

type ProtocolCharacteristicSignature = (
    String, String, String, Option<(String, u64, u64, u64, bool)>,
);

fn protocol_characteristic_signature(session: &Session) -> Vec<ProtocolCharacteristicSignature> {
    let values = session.measurements.iter().map(|measurement| {
        let reference = session.reference_document.as_ref().and_then(|document| document.dimensions.iter()
            .find(|dimension| dimension.measurement_id == Some(measurement.id)))
            .map(|dimension| (
                dimension.label.trim().to_lowercase(),
                dimension.nominal_mm.to_bits(),
                dimension.tolerance_minus_mm.to_bits(),
                dimension.tolerance_plus_mm.to_bits(),
                dimension.confirmed,
            ));
        (
            if measurement.label.trim().is_empty() { format!("M{}", measurement.id).to_lowercase() }
                else { measurement.label.trim().to_lowercase() },
            measurement.kind.label().to_owned(),
            measurement.method.as_deref().unwrap_or_default().trim().to_lowercase(),
            reference,
        )
    });
    let mut values = values.collect::<Vec<_>>();
    values.sort();
    values
}

fn protocol_characteristic_signature_from_protocol(
    characteristics: &[storage::ProtocolCharacteristic],
) -> Vec<ProtocolCharacteristicSignature> {
    let mut values = characteristics.iter().map(|characteristic| {
        let reference = characteristic.reference.as_ref().map(|dimension| (
            dimension.label.trim().to_lowercase(),
            dimension.nominal_mm.to_bits(),
            dimension.tolerance_minus_mm.to_bits(),
            dimension.tolerance_plus_mm.to_bits(),
            dimension.confirmed,
        ));
        (
            characteristic.label.trim().to_lowercase(),
            characteristic.kind.clone(),
            characteristic.method.as_deref().unwrap_or_default().trim().to_lowercase(),
            reference,
        )
    }).collect::<Vec<_>>();
    values.sort();
    values
}

fn empty_session(name: String, image_file: String) -> Session {
    Session {
        schema_version: storage::CURRENT_SCHEMA_VERSION,
        capture_id: format!("photo-{name}"), image_file,
        device: "Image importée".into(), device_id: "imported-image".into(), optical_setup: String::new(),
        calibration: None, calibrations: Vec::new(), measurements: Vec::new(),
        annotations: Vec::new(), quality_thresholds: Default::default(),
        quality_assessment: None, display_correction: Default::default(),
        protocol: None,
        reference_document: None,
        measurement_audit_history: Vec::new(),
    }
}

fn session_is_valid(session: &Session, width: u32, height: u32) -> bool {
    if session.schema_version > storage::CURRENT_SCHEMA_VERSION { return false; }
    let point_is_valid = |point: &crate::geometry::Point| {
        point.x.is_finite() && point.y.is_finite()
            && point.x >= 0.0 && point.y >= 0.0
            && point.x <= width as f64 && point.y <= height as f64
    };
    if crate::quality::assess_image_quality(
        &[0; 27], 3, 3, &session.quality_thresholds,
    ).is_err() || crate::quality::corrected_display_rgb(&[], session.display_correction).is_err() {
        return false;
    }
    let valid_profile = |profile: &storage::Calibration| {
        profile.resolution == [width, height]
            && profile.mm_per_pixel.is_finite() && profile.mm_per_pixel > 0.0
            && profile.scale_x_mm_per_pixel.is_none_or(|scale| scale.is_finite() && scale > 0.0)
            && profile.scale_y_mm_per_pixel.is_none_or(|scale| scale.is_finite() && scale > 0.0)
            && profile.reference_mm.is_finite() && profile.reference_mm > 0.0
            && profile.reference_px.is_finite() && profile.reference_px > 0.0
            && profile.expanded_uncertainty_mm
                .is_none_or(|uncertainty| uncertainty.is_finite() && uncertainty > 0.0)
            && profile.verification_residual_mm.is_none_or(f64::is_finite)
            && ((profile.corner_verification_residuals_mm.is_empty()
                && profile.additional_corner_validation_segments_native_px.is_empty())
                || (profile.corner_verification_residuals_mm.len() == 4
                    && profile.additional_corner_validation_segments_native_px.len() == 3))
            && profile.corner_verification_residuals_mm.iter().all(|residual| residual.is_finite())
            && profile.validation_points_native_px.iter().all(|point| {
                point.x.is_finite() && point.y.is_finite()
                    && point.x >= 0.0 && point.y >= 0.0
                    && point.x <= width as f64 && point.y <= height as f64
            })
            && profile.additional_corner_validation_segments_native_px.iter().flatten()
                .all(|point| point.x.is_finite() && point.y.is_finite()
                    && point.x >= 0.0 && point.y >= 0.0
                    && point.x <= width as f64 && point.y <= height as f64)
            && !profile.id.is_empty()
    };
    if session.calibration.as_ref().is_some_and(|profile| !valid_profile(profile))
        || session.calibrations.iter().any(|profile| !valid_profile(profile))
    { return false; }
    if session.protocol.as_ref().is_some_and(|protocol| {
        protocol.id.trim().is_empty() || protocol.version == 0
            || protocol.resolution.contains(&0)
            || protocol.mm_per_pixel.is_some_and(|scale| !scale.is_finite() || scale <= 0.0)
            || protocol.scale_x_mm_per_pixel.is_some_and(|scale| !scale.is_finite() || scale <= 0.0)
            || protocol.scale_y_mm_per_pixel.is_some_and(|scale| !scale.is_finite() || scale <= 0.0)
            || protocol.expanded_uncertainty_mm
                .is_some_and(|uncertainty| !uncertainty.is_finite() || uncertainty <= 0.0)
            || protocol.expected_measurements.iter().any(|characteristic| {
                characteristic.label.trim().is_empty() || characteristic.kind.trim().is_empty()
                    || characteristic.reference.as_ref().is_some_and(|dimension| {
                        !dimension.nominal_mm.is_finite() || dimension.nominal_mm <= 0.0
                            || !dimension.tolerance_minus_mm.is_finite()
                            || dimension.tolerance_minus_mm < 0.0
                            || !dimension.tolerance_plus_mm.is_finite()
                            || dimension.tolerance_plus_mm < 0.0
                    })
            })
    }) { return false; }
    if session.measurements.iter().any(|measurement| {
        measurement.protocol_id.is_some() != measurement.protocol_version.is_some()
            || measurement.protocol_id.as_ref().is_some_and(|id| id.trim().is_empty())
    }) { return false; }
    if session.annotations.iter().any(|annotation| {
        annotation.text.trim().is_empty() || annotation.text.chars().count() > 2000
            || match &annotation.target {
                crate::geometry::AnnotationTarget::Measurement { measurement_id } =>
                    !session.measurements.iter().any(|measurement| measurement.id == *measurement_id),
                crate::geometry::AnnotationTarget::Point { point } => !point_is_valid(point),
                crate::geometry::AnnotationTarget::Zone { first, second } =>
                    !point_is_valid(first) || !point_is_valid(second),
            }
    }) { return false; }
    let calibration_exists = |id: &str| {
        session.calibrations.iter().any(|profile| profile.id == id)
            || session.calibration.as_ref().is_some_and(|profile| profile.id == id)
    };
    session.measurements.iter().all(|measurement| {
        measurement.mm_per_pixel.is_none_or(|scale| scale.is_finite() && scale > 0.0)
            && measurement.mm_per_pixel_x.is_none_or(|scale| scale.is_finite() && scale > 0.0)
            && measurement.mm_per_pixel_y.is_none_or(|scale| scale.is_finite() && scale > 0.0)
            && measurement.expanded_uncertainty_mm
                .is_none_or(|uncertainty| uncertainty.is_finite() && uncertainty > 0.0)
            && (measurement.capture_id.is_empty() || measurement.capture_id == session.capture_id)
            && (session.schema_version < 3 || measurement.calibration_id.as_ref().is_none_or(|id| calibration_exists(id)))
            && measurement.calibration_id.as_ref().is_none_or(|id| {
                session.calibrations.iter().chain(session.calibration.iter())
                    .find(|profile| &profile.id == id).is_some_and(|profile| profile.validation_status != "failed")
            })
            && measurement.points.iter().all(point_is_valid)
            && measurement.label_position_native_px.as_ref().is_none_or(point_is_valid)
            && crate::geometry::is_nondegenerate(measurement.kind, &measurement.points)
    })
}

fn load_sessions(folder: &Path) -> Vec<(PathBuf, Session)> {
    let Ok(entries) = fs::read_dir(folder) else { return Vec::new(); };
    let mut candidates = entries.filter_map(Result::ok).map(|entry| entry.path())
        .filter(|path| {
            matches!(path.extension().and_then(|ext| ext.to_str()).map(str::to_ascii_lowercase).as_deref(),
                Some("dperproj" | "json"))
                && path.file_name().and_then(|name| name.to_str()) != Some(PREFERENCES_FILE)
        }).collect::<Vec<_>>();
    candidates.sort();
    candidates.into_iter().filter_map(|session_path| {
        let body = fs::read(&session_path).ok()?;
        let session: Session = serde_json::from_slice(&body).ok()?;
        if session.image_file.trim().is_empty() || session.schema_version > storage::CURRENT_SCHEMA_VERSION {
            return None;
        }
        let referenced = PathBuf::from(&session.image_file);
        let source = if referenced.is_absolute() { referenced }
            else { session_path.parent().unwrap_or(folder).join(referenced) };
        Some((canonical(&source)?, session))
    }).collect()
}

fn is_supported_image(path: &Path) -> bool {
    matches!(path.extension().and_then(|ext| ext.to_str()).map(str::to_ascii_lowercase).as_deref(),
        Some("png" | "jpg" | "jpeg"))
}

fn canonical(path: &Path) -> Option<PathBuf> {
    fs::canonicalize(path).ok()
}

fn thumbnail_from_rgb(
    ctx: &egui::Context, rgb: &[u8], width: u32, height: u32, key: &str,
) -> Option<egui::TextureHandle> {
    let image = image::RgbImage::from_raw(width, height, rgb.to_vec())?;
    let thumbnail = image::DynamicImage::ImageRgb8(image).thumbnail(112, 72).to_rgb8();
    let (width, height) = thumbnail.dimensions();
    let color = egui::ColorImage::from_rgb([width as usize, height as usize], thumbnail.as_raw());
    Some(ctx.load_texture(format!("export-thumb-{key}"), color, egui::TextureOptions::LINEAR))
}

fn format_timestamp(time: SystemTime) -> String {
    let seconds = time.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Measurement, Point, Tool};

    fn series_row(index: usize) -> PhotoRow {
        let mut session = empty_session(format!("piece-{index}"), format!("piece-{index}.png"));
        session.device = "Caméra de test".into();
        session.device_id = "camera-test-id".into();
        session.optical_setup = "Objectif 1".into();
        session.measurements.push(Measurement {
            id: 1,
            kind: Tool::Distance,
            points: vec![Point { x: 0.0, y: 0.0 }, Point { x: 10.0, y: 0.0 }],
            label: "Longueur A".into(),
            label_position_native_px: None,
            capture_id: format!("capture-{index}"),
            calibration_id: Some("cal-1".into()),
            mm_per_pixel: Some(0.01),
            mm_per_pixel_x: None,
            mm_per_pixel_y: None,
            expanded_uncertainty_mm: Some(0.02),
            origin: "test".into(),
            method: None,
            protocol_id: Some("protocol-1".into()),
            protocol_version: Some(1),
        });
        session.calibration = Some(storage::Calibration {
            id: "cal-1".into(), device: "Caméra de test".into(),
            device_id: "camera-test-id".into(), resolution: [4, 4],
            optical_setup: "Objectif 1".into(), mm_per_pixel: 0.01,
            scale_x_mm_per_pixel: None, scale_y_mm_per_pixel: None,
            expanded_uncertainty_mm: Some(0.02), uncertainty_budget: None,
            reference_mm: 1.0, reference_px: 100.0,
            reference_x_mm: None, reference_y_mm: None,
            verification_reference_mm: None, verification_residual_mm: None,
            corner_verification_residuals_mm: Vec::new(),
            validation_status: "verified".into(),
            validation_points_native_px: [Point::default(); 2],
            additional_corner_validation_segments_native_px: Vec::new(),
            author: String::new(), reference_image: String::new(),
            reference_type: "segment".into(),
            reference_points_native_px: [Point::default(); 2],
            secondary_reference_points_native_px: [Point::default(); 2],
            origin: "test".into(), created_at_unix_secs: 0,
        });
        session.protocol = Some(storage::MeasurementProtocol {
            id: "protocol-1".into(), name: "Contrôle".into(), version: 1,
            part_family: "Famille A".into(), device: "Caméra de test".into(),
            device_id: "camera-test-id".into(), optical_setup: "Objectif 1".into(),
            resolution: [4, 4], calibration_profile_id: Some("cal-1".into()),
            mm_per_pixel: Some(0.01), scale_x_mm_per_pixel: None, scale_y_mm_per_pixel: None,
            expanded_uncertainty_mm: Some(0.02),
            quality_thresholds: Default::default(), display_correction: Default::default(),
            expected_measurements: vec![storage::ProtocolCharacteristic {
                label: "Longueur A".into(), kind: Tool::Distance.label().into(),
                method: None, reference: None,
            }],
            display_precision_mm: 3,
            tolerance_rule: storage::ToleranceRule::ExpandedUncertaintyGuardBand,
            created_at_unix_secs: 0,
        });
        PhotoRow {
            key: format!("piece-{index}"), name: format!("piece-{index}.png"),
            description: String::new(), timestamp: None, include_timestamp: false,
            selected: true, path: None, current_rgb: None, width: 4, height: 4,
            session, thumbnail: None, error: None, warning: None,
        }
    }

    fn attach_test_image(row: &mut PhotoRow) {
        let mut rgb = Vec::with_capacity(16 * 16 * 3);
        for y in 0..16 {
            for x in 0..16 {
                let value = if (x + y) % 2 == 0 { 16 } else { 240 };
                rgb.extend_from_slice(&[value, value, value]);
            }
        }
        row.width = 16;
        row.height = 16;
        row.current_rgb = Some(rgb.clone());
        row.session.calibration.as_mut().unwrap().resolution = [16, 16];
        let thresholds = crate::quality::ImageQualityThresholds {
            min_mean_luminance: 0.0,
            max_mean_luminance: 255.0,
            min_contrast_std_dev: 0.0,
            min_laplacian_variance: 0.0,
            min_edge_visibility_fraction: 0.0,
            max_clipped_pixel_fraction: 1.0,
            edge_gradient_threshold: 0.0,
        };
        row.session.quality_thresholds = thresholds.clone();
        let protocol = row.session.protocol.as_mut().unwrap();
        protocol.resolution = [16, 16];
        protocol.quality_thresholds = thresholds.clone();
        let assessment = crate::quality::assess_image_quality_with_correction(
            &rgb, 16, 16, &thresholds, row.session.display_correction,
        ).unwrap();
        assert!(assessment.is_exploitable);
        row.session.quality_assessment = Some(assessment);
    }

    #[test]
    fn five_piece_series_stays_comparable_until_a_settings_mismatch() {
        let mut rows = (0..5).map(series_row).collect::<Vec<_>>();
        let references = rows.iter().collect::<Vec<_>>();
        assert_eq!(report_series_notice(&references), None);

        rows[4].session.display_correction.gamma = 1.1;
        let mismatched = rows.iter().collect::<Vec<_>>();
        let notice = report_series_notice(&mismatched).unwrap();
        assert!(notice.contains("Verdict de série suspendu"));
        assert!(notice.contains("corrections d'affichage différentes"));
    }

    #[test]
    fn ab_comparison_reports_deltas_and_suspends_on_changed_conditions() {
        let mut a = series_row(0);
        let mut b = series_row(1);
        attach_test_image(&mut a);
        attach_test_image(&mut b);
        b.session.measurements[0].points[1].x = 12.0;

        let comparison = crate::comparison::compare_sessions(
            &a.session, &b.session, a.name.clone(), b.name.clone(),
        );
        assert!(comparison.comparable, "{:?}", comparison.reasons);
        assert_eq!(comparison.rows.len(), 1);
        assert_eq!(comparison.rows[0].value_a_mm, Some(0.1));
        assert_eq!(comparison.rows[0].value_b_mm, Some(0.12));
        assert!((comparison.rows[0].delta_b_minus_a_mm.unwrap() - 0.02).abs() < 1e-12);
        assert!(String::from_utf8(crate::comparison::to_csv(&comparison).unwrap())
            .unwrap().contains("0.02000000"));

        b.session.display_correction.gamma = 1.1;
        let rgb = b.current_rgb.as_ref().unwrap();
        b.session.quality_assessment = Some(crate::quality::assess_image_quality_with_correction(
            rgb, b.width, b.height, &b.session.quality_thresholds, b.session.display_correction,
        ).unwrap());
        let changed = crate::comparison::compare_sessions(
            &a.session, &b.session, a.name, b.name,
        );
        assert!(!changed.comparable);
        assert!(changed.reasons.iter().any(|reason| reason.contains("Correction d'affichage différente")));
    }

    #[test]
    fn batch_exports_include_only_selected_images_with_measurements() {
        let mut first = series_row(0);
        let mut second = series_row(1);
        let mut empty = series_row(2);
        attach_test_image(&mut first);
        attach_test_image(&mut second);
        empty.session.measurements.clear();
        let dialog = Dialog {
            folder: PathBuf::new(),
            preferences: Preferences::default(),
            rows: vec![first, second, empty],
        };

        assert_eq!(dialog.selected_count(), 2);
        let csv = String::from_utf8(dialog.measurements_csv_batch().unwrap()).unwrap();
        assert!(csv.contains("piece-0.png"));
        assert!(csv.contains("piece-1.png"));
        assert!(!csv.contains("piece-2.png"));
        assert_eq!(csv.matches("Longueur A").count(), 2);

        let xlsx = dialog.measurements_xlsx_batch().unwrap();
        assert!(xlsx.starts_with(b"PK\x03\x04"));
        let xlsx_contents = String::from_utf8_lossy(&xlsx);
        assert!(xlsx_contents.contains("piece-0.png"));
        assert!(xlsx_contents.contains("piece-1.png"));
        assert!(!xlsx_contents.contains("piece-2.png"));
        assert_eq!(xlsx_contents.matches("Longueur A").count(), 2);
    }
}
