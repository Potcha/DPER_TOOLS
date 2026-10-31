//! Local command adapters for PhotoCraft image correction and SolidCraft sketches.
use crate::{exports, imaging, quality, storage};
use image::{DynamicImage, ImageFormat, RgbImage};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::{Cursor, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_IMAGE_PIXELS: u64 = 12_000_000;
const MAX_RECIPE_BYTES: u64 = 1_000_000;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PhotoCraftRecipe {
    schema_version: u32,
    #[serde(default)]
    flat_field_image: Option<String>,
    #[serde(default)]
    dark_frame_image: Option<String>,
    #[serde(default)]
    radial_distortion_profile: Option<String>,
    #[serde(default)]
    display_correction: Option<DisplayCorrectionRecipe>,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
struct DisplayCorrectionRecipe {
    brightness: f32,
    contrast: f32,
    gamma: f32,
}

impl Default for DisplayCorrectionRecipe {
    fn default() -> Self {
        Self { brightness: 0.0, contrast: 1.0, gamma: 1.0 }
    }
}

impl From<DisplayCorrectionRecipe> for quality::DisplayCorrection {
    fn from(value: DisplayCorrectionRecipe) -> Self {
        Self { brightness: value.brightness, contrast: value.contrast, gamma: value.gamma }
    }
}

pub fn run(arguments: &[String]) -> i32 {
    let result = match arguments.first().map(String::as_str) {
        Some("drawing-geometry") if arguments.len() == 4 => {
            arguments[2].parse::<u32>().map_err(|_| "Page PDF invalide.".to_owned())
                .and_then(|page| crate::dper_pdf_geometry::analyze_pdf_geometry(Path::new(&arguments[1]), page))
                .and_then(|catalog| {
                    let content = serde_json::to_vec_pretty(&catalog).map_err(|error| error.to_string())?;
                    write_new_file(Path::new(&arguments[3]), &content)?;
                    Ok(format!("{} primitives candidates; catalogue : {}", catalog.geometry.primitives.len(), arguments[3]))
                })
        }
        Some("drawing-plan") if arguments.len() == 3 => {
            let plan = crate::dper_drawing::analyze_path(Path::new(&arguments[1]));
            plan.and_then(|plan| {
                let content = serde_json::to_vec_pretty(&plan).map_err(|error| error.to_string())?;
                write_new_file(Path::new(&arguments[2]), &content)?;
                Ok(format!("{} caractéristiques candidates; plan de contrôle : {}", plan.characteristics.len(), arguments[2]))
            })
        }
        Some("photocraft-correct") if (3..=4).contains(&arguments.len()) => {
            run_photocraft_correct(&arguments[1..])
        }
        Some("solidcraft-sketch") if (2..=3).contains(&arguments.len()) => {
            run_solidcraft_sketch(&arguments[1..])
        }
        _ => {
            print_usage();
            return 2;
        }
    };
    match result {
        Ok(message) => { println!("{message}"); 0 }
        Err(error) => { eprintln!("{error}"); 1 }
    }
}

fn run_photocraft_correct(arguments: &[String]) -> Result<String, String> {
    let source_path = canonical_file(Path::new(&arguments[0]), "Image source")?;
    let recipe_path = canonical_file(Path::new(&arguments[1]), "Recette PhotoCraft")?;
    let recipe_bytes = read_limited(&recipe_path, MAX_RECIPE_BYTES, "Recette PhotoCraft")?;
    let recipe: PhotoCraftRecipe = serde_json::from_slice(&recipe_bytes)
        .map_err(|error| format!("Recette PhotoCraft illisible : {error}"))?;
    if recipe.schema_version != 1 {
        return Err(format!("Version de recette PhotoCraft non prise en charge : {}", recipe.schema_version));
    }
    if recipe.flat_field_image.is_none() && recipe.radial_distortion_profile.is_none()
        && recipe.display_correction.is_none()
    {
        return Err("La recette ne demande aucune correction d'image.".into());
    }
    if recipe.dark_frame_image.is_some() && recipe.flat_field_image.is_none() {
        return Err("Une image noire ne peut être utilisée qu'avec une correction flat-field.".into());
    }

    let recipe_dir = recipe_path.parent().unwrap_or_else(|| Path::new("."));
    let flat_field_path = recipe.flat_field_image.as_deref()
        .map(|path| canonical_file(&resolve_path(recipe_dir, path), "Image flat-field"))
        .transpose()?;
    let dark_frame_path = recipe.dark_frame_image.as_deref()
        .map(|path| canonical_file(&resolve_path(recipe_dir, path), "Image noire"))
        .transpose()?;
    let distortion_profile_path = recipe.radial_distortion_profile.as_deref()
        .map(|path| canonical_file(&resolve_path(recipe_dir, path), "Profil de distorsion"))
        .transpose()?;

    let mut derived = decode_rgb(&source_path)?;
    let [width, height] = [derived.width(), derived.height()];
    let mut transform_order = Vec::<&str>::new();
    if let Some(path) = flat_field_path.as_ref() {
        let flat_field = decode_rgb(path)?;
        let dark_frame = dark_frame_path.as_ref().map(|path| decode_rgb(path.as_path())).transpose()?;
        derived = imaging::flat_field_correct(&derived, &flat_field, dark_frame.as_ref())?;
        transform_order.push("flat_field");
    }
    let mut distortion_profile_json = None;
    if let Some(path) = distortion_profile_path.as_ref() {
        let profile_bytes = read_limited(path, MAX_RECIPE_BYTES, "Profil de distorsion")?;
        let profile_value: serde_json::Value = serde_json::from_slice(&profile_bytes)
            .map_err(|error| format!("Profil de distorsion illisible : {error}"))?;
        let profile: imaging::RadialDistortionCalibration = serde_json::from_value(profile_value.clone())
            .map_err(|error| format!("Profil de distorsion invalide : {error}"))?;
        derived = imaging::radial_distortion_correct(&derived, &profile)?;
        distortion_profile_json = Some(profile_value);
        transform_order.push("radial_distortion");
    }
    if let Some(correction) = recipe.display_correction {
        let rgb = quality::corrected_display_rgb(derived.as_raw(), correction.into())
            .map_err(str::to_owned)?;
        derived = RgbImage::from_raw(width, height, rgb)
            .ok_or_else(|| "Buffer corrigé incompatible avec les dimensions de l'image.".to_owned())?;
        transform_order.push("brightness_contrast_gamma");
    }

    let output_path = arguments.get(2).map(PathBuf::from).unwrap_or_else(|| {
        let stem = source_path.file_stem().and_then(|value| value.to_str()).unwrap_or("image");
        source_path.with_file_name(format!("{stem}.photocraft.png"))
    });
    require_extension(&output_path, "png", "Image de sortie PhotoCraft")?;
    let mut png = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(derived).write_to(&mut png, ImageFormat::Png)
        .map_err(|error| format!("Encodage PNG impossible : {error}"))?;

    let manifest = serde_json::json!({
        "format_version": 1,
        "producer": "DPER_MEASURE",
        "producer_version": env!("CARGO_PKG_VERSION"),
        "operation": "photocraft_image_correction",
        "created_at_unix_secs": SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
        "source_image": source_path.display().to_string(),
        "recipe_file": recipe_path.display().to_string(),
        "recipe": recipe,
        "flat_field_image": flat_field_path.as_ref().map(|path| path.display().to_string()),
        "dark_frame_image": dark_frame_path.as_ref().map(|path| path.display().to_string()),
        "radial_distortion_profile_file": distortion_profile_path.as_ref()
            .map(|path| path.display().to_string()),
        "radial_distortion_profile": distortion_profile_json,
        "transform_order": transform_order,
        "output_image": output_path.file_name().and_then(|value| value.to_str()).unwrap_or("image.png"),
        "output_dimensions_px": [width, height],
        "source_image_modified": false,
        "source_calibration_reuse": "not_allowed_recalibrate_derived_image",
        "note": "La correction produit une nouvelle image; établissez son échelle avant toute mesure.",
    });
    let manifest_path = write_artifact_pair(&output_path, &png.into_inner(), &manifest)?;
    Ok(format!("Image PhotoCraft créée : {} · manifeste : {}",
        output_path.display(), manifest_path.display()))
}

fn run_solidcraft_sketch(arguments: &[String]) -> Result<String, String> {
    let project_path = canonical_file(Path::new(&arguments[0]), "Projet DPER_MEASURE")?;
    let project_bytes = fs::read(&project_path)
        .map_err(|error| format!("Lecture du projet impossible : {error}"))?;
    let session: storage::Session = serde_json::from_slice(&project_bytes)
        .map_err(|error| format!("Projet DPER_MEASURE illisible : {error}"))?;
    if session.schema_version > storage::CURRENT_SCHEMA_VERSION {
        return Err(format!("Version de projet non prise en charge : {}", session.schema_version));
    }
    if session.measurements.is_empty() {
        return Err("Le projet ne contient aucune géométrie à convertir en esquisse.".into());
    }
    if session.image_file.trim().is_empty() {
        return Err("Le projet ne référence aucune image source permettant d'orienter le DXF.".into());
    }
    let image_path = PathBuf::from(&session.image_file);
    let image_path = if image_path.is_absolute() { image_path }
        else { project_path.parent().unwrap_or_else(|| Path::new(".")).join(image_path) };
    let image_path = canonical_file(&image_path, "Image source du projet")?;
    let (width, height) = image_dimensions(&image_path)?;
    if width == 0 || height == 0 {
        return Err("L'image source du projet ne possède pas de dimensions utilisables.".into());
    }
    validate_project_resolution(&session, [width, height])?;
    let dxf = exports::measurement_dxf(&session, height)?;
    let output_path = arguments.get(1).map(PathBuf::from).unwrap_or_else(|| {
        let stem = project_path.file_stem().and_then(|value| value.to_str()).unwrap_or("inspection");
        project_path.with_file_name(format!("{stem}.solidcraft.dxf"))
    });
    require_extension(&output_path, "dxf", "Esquisse de sortie SolidCraft")?;
    let measurement_provenance = session.measurements.iter().map(|measurement| serde_json::json!({
        "measurement_id": measurement.id,
        "label": measurement.label,
        "type": measurement.kind.label(),
        "capture_id": measurement.capture_id,
        "calibration_profile_id": measurement.calibration_id,
        "scale_x_mm_per_pixel": measurement.mm_per_pixel_x.or(measurement.mm_per_pixel),
        "scale_y_mm_per_pixel": measurement.mm_per_pixel_y.or(measurement.mm_per_pixel),
        "expanded_uncertainty_mm": measurement.expanded_uncertainty_mm,
        "points_native_px": measurement.points,
    })).collect::<Vec<_>>();
    let mut used_profile_ids = BTreeSet::new();
    let calibration_provenance = session.measurements.iter().filter_map(|measurement| {
        let profile = measurement.calibration_id.as_deref()
            .and_then(|id| session.calibration.as_ref().filter(|profile| profile.id == id)
                .or_else(|| session.calibrations.iter().find(|profile| profile.id == id)))
            .or_else(|| session.calibration.as_ref())?;
        if !used_profile_ids.insert(profile.id.clone()) { return None; }
        Some(serde_json::json!({
            "id": profile.id,
            "device_id": profile.device_id,
            "resolution_px": profile.resolution,
            "optical_setup": profile.optical_setup,
            "scale_x_mm_per_pixel": profile.scale_x_mm_per_pixel.or(Some(profile.mm_per_pixel)),
            "scale_y_mm_per_pixel": profile.scale_y_mm_per_pixel.or(Some(profile.mm_per_pixel)),
            "expanded_uncertainty_mm": profile.expanded_uncertainty_mm,
            "validation_status": profile.validation_status,
            "reference_type": profile.reference_type,
            "author": profile.author,
            "created_at_unix_secs": profile.created_at_unix_secs,
        }))
    }).collect::<Vec<_>>();
    let manifest = serde_json::json!({
        "format_version": 1,
        "producer": "DPER_MEASURE",
        "producer_version": env!("CARGO_PKG_VERSION"),
        "operation": "solidcraft_sketch_export",
        "created_at_unix_secs": SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
        "source_project": project_path.display().to_string(),
        "source_image": image_path.display().to_string(),
        "capture_id": session.capture_id,
        "measurement_count": session.measurements.len(),
        "calibration_profile_id": session.calibration.as_ref().map(|profile| profile.id.as_str()),
        "calibration_status": session.calibration.as_ref().map(|profile| profile.validation_status.as_str()),
        "calibration_profiles": calibration_provenance,
        "image_dimensions_px": [width, height],
        "drawing_format": "DXF R2010",
        "drawing_units": "millimeter",
        "output_dxf": output_path.file_name().and_then(|value| value.to_str()).unwrap_or("sketch.dxf"),
        "measurement_provenance": measurement_provenance,
        "note": "Vérifier les unités, l'orientation et les entités dans SolidCraft avant fabrication.",
    });
    let manifest_path = write_artifact_pair(&output_path, &dxf, &manifest)?;
    Ok(format!("Esquisse SolidCraft créée : {} · manifeste : {}",
        output_path.display(), manifest_path.display()))
}

fn decode_rgb(path: &Path) -> Result<RgbImage, String> {
    let dimensions = image::ImageReader::open(path)
        .map_err(|error| format!("Ouverture de {} impossible : {error}", path.display()))?
        .with_guessed_format()
        .map_err(|error| format!("Format de {} illisible : {error}", path.display()))?
        .into_dimensions()
        .map_err(|error| format!("Dimensions de {} illisibles : {error}", path.display()))?;
    ensure_image_dimensions(dimensions.0, dimensions.1, path)?;
    let decoded = image::ImageReader::open(path)
        .map_err(|error| format!("Ouverture de {} impossible : {error}", path.display()))?
        .with_guessed_format()
        .map_err(|error| format!("Format de {} illisible : {error}", path.display()))?
        .decode()
        .map_err(|error| format!("Décodage de {} impossible : {error}", path.display()))?;
    Ok(decoded.to_rgb8())
}

fn image_dimensions(path: &Path) -> Result<(u32, u32), String> {
    image::ImageReader::open(path)
        .map_err(|error| format!("Ouverture de {} impossible : {error}", path.display()))?
        .with_guessed_format()
        .map_err(|error| format!("Format de {} illisible : {error}", path.display()))?
        .into_dimensions()
        .map_err(|error| format!("Dimensions de {} illisibles : {error}", path.display()))
}

fn validate_project_resolution(session: &storage::Session, resolution: [u32; 2]) -> Result<(), String> {
    if let Some(protocol) = session.protocol.as_ref() {
        if protocol.resolution != resolution {
            return Err(format!("Résolution source {:?} différente de celle du protocole {:?}; esquisse annulée.",
                resolution, protocol.resolution));
        }
    }
    let profile_for_id = |id: &str| session.calibration.as_ref()
        .filter(|profile| profile.id == id)
        .or_else(|| session.calibrations.iter().find(|profile| profile.id == id));
    for measurement in &session.measurements {
        let profile = measurement.calibration_id.as_deref().map(|id| {
            profile_for_id(id).ok_or_else(|| format!(
                "Profil {} absent du projet; esquisse M{} annulée.", id, measurement.id))
        }).transpose()?;
        let profile = profile.or_else(|| session.calibration.as_ref());
        if let Some(profile) = profile {
            if profile.validation_status == "failed" {
                return Err(format!("Le profil d'étalonnage de M{} a échoué au contrôle; esquisse annulée.",
                    measurement.id));
            }
            if profile.resolution != resolution {
                return Err(format!("Résolution source {:?} différente de celle du profil de M{} {:?}; esquisse annulée.",
                    resolution, measurement.id, profile.resolution));
            }
        }
    }
    Ok(())
}

fn ensure_image_dimensions(width: u32, height: u32, path: &Path) -> Result<(), String> {
    let valid_dimensions = u64::from(width).checked_mul(u64::from(height))
        .filter(|pixels| *pixels > 0 && *pixels <= MAX_IMAGE_PIXELS)
        .is_some();
    if valid_dimensions { Ok(()) }
    else {
        Err(format!("{} dépasse la limite locale de {} millions de pixels.",
            path.display(), MAX_IMAGE_PIXELS / 1_000_000))
    }
}

fn canonical_file(path: &Path, label: &str) -> Result<PathBuf, String> {
    let canonical = fs::canonicalize(path)
        .map_err(|error| format!("{label} introuvable ou inaccessible ({}): {error}", path.display()))?;
    if !canonical.is_file() { return Err(format!("{label} n'est pas un fichier : {}", canonical.display())); }
    Ok(canonical)
}

fn resolve_path(base: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() { path.to_path_buf() } else { base.join(path) }
}

fn read_limited(path: &Path, maximum_bytes: u64, label: &str) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("Lecture des métadonnées de {label} impossible : {error}"))?;
    if metadata.len() > maximum_bytes {
        return Err(format!("{label} dépasse la taille maximale de {} Ko.", maximum_bytes / 1024));
    }
    fs::read(path).map_err(|error| format!("Lecture de {label} impossible : {error}"))
}

fn require_extension(path: &Path, expected: &str, label: &str) -> Result<(), String> {
    if path.extension().and_then(|value| value.to_str())
        .is_none_or(|extension| !extension.eq_ignore_ascii_case(expected))
    {
        return Err(format!("{label} doit utiliser l'extension .{expected}."));
    }
    Ok(())
}

fn write_artifact_pair(output: &Path, content: &[u8], manifest: &serde_json::Value) -> Result<PathBuf, String> {
    let sidecar = output.with_extension("dper_measure.json");
    if output.exists() { return Err(format!("Le fichier de sortie existe déjà : {}", output.display())); }
    if sidecar.exists() { return Err(format!("Le manifeste existe déjà : {}", sidecar.display())); }
    if let Some(parent) = output.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Création du dossier de sortie impossible : {error}"))?;
    }
    let manifest_bytes = serde_json::to_vec_pretty(manifest)
        .map_err(|error| format!("Sérialisation du manifeste impossible : {error}"))?;
    write_new_file(output, content)?;
    if let Err(error) = write_new_file(&sidecar, &manifest_bytes) {
        let _ = fs::remove_file(output);
        return Err(format!("Écriture du manifeste impossible; l'artefact incomplet a été retiré : {error}"));
    }
    Ok(sidecar)
}

fn write_new_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)
        .map_err(|error| format!("Création de {} impossible : {error}", path.display()))?;
    if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(format!("Écriture de {} impossible : {error}", path.display()));
    }
    Ok(())
}

fn print_usage() {
    eprintln!("       dper_measure --cli drawing-plan <plan.pdf> <sortie.json>");
    eprintln!("       dper_measure --cli drawing-geometry <plan.pdf> <page> <sortie.json>");
    eprintln!("Usage : dper_measure --cli photocraft-correct <image> <recette.json> [sortie.png]");
    eprintln!("       dper_measure --cli solidcraft-sketch <session.dperproj|session.json> [sortie.dxf]");
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
            let path = std::env::temp_dir()
                .join(format!("dper-measure-cli-test-{}-{stamp}", std::process::id()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
    }

    #[test]
    fn photocraft_derives_a_png_with_manifest_and_refuses_overwrite() {
        let directory = TestDirectory::new();
        let source_path = directory.0.join("source.png");
        let recipe_path = directory.0.join("recipe.json");
        let output_path = directory.0.join("derived.png");
        let source = RgbImage::from_pixel(2, 2, image::Rgb([100, 100, 100]));
        source.save(&source_path).unwrap();
        fs::write(&recipe_path,
            br#"{"schema_version":1,"display_correction":{"brightness":20.0}}"#).unwrap();
        let original_bytes = fs::read(&source_path).unwrap();

        run_photocraft_correct(&[
            source_path.to_string_lossy().into_owned(),
            recipe_path.to_string_lossy().into_owned(),
            output_path.to_string_lossy().into_owned(),
        ]).unwrap();

        let derived = image::open(&output_path).unwrap().to_rgb8();
        assert_eq!(derived.get_pixel(0, 0), &image::Rgb([120, 120, 120]));
        assert_eq!(fs::read(&source_path).unwrap(), original_bytes);
        let manifest_path = output_path.with_extension("dper_measure.json");
        let manifest: serde_json::Value = serde_json::from_slice(&fs::read(manifest_path).unwrap()).unwrap();
        assert_eq!(manifest["source_image_modified"], false);
        assert_eq!(manifest["source_calibration_reuse"], "not_allowed_recalibrate_derived_image");
        assert!(manifest["transform_order"][0] == "brightness_contrast_gamma");

        assert!(run_photocraft_correct(&[
            source_path.to_string_lossy().into_owned(),
            recipe_path.to_string_lossy().into_owned(),
            output_path.to_string_lossy().into_owned(),
        ]).unwrap_err().contains("existe déjà"));
    }

    #[test]
    fn solidcraft_exports_a_calibrated_project_and_provenance_manifest() {
        let directory = TestDirectory::new();
        let source_path = directory.0.join("source.png");
        let project_path = directory.0.join("project.json");
        let output_path = directory.0.join("sketch.dxf");
        RgbImage::from_pixel(32, 24, image::Rgb([90, 90, 90])).save(&source_path).unwrap();
        let project = serde_json::json!({
            "schema_version": crate::storage::CURRENT_SCHEMA_VERSION,
            "capture_id": "capture-solidcraft",
            "image_file": "source.png",
            "measurements": [{
                "id": 1,
                "kind": "Distance",
                "points": [{"x": 2.0, "y": 3.0}, {"x": 12.0, "y": 3.0}],
                "label": "Largeur",
                "capture_id": "capture-solidcraft",
                "mm_per_pixel": 0.1
            }]
        });
        fs::write(&project_path, serde_json::to_vec(&project).unwrap()).unwrap();

        run_solidcraft_sketch(&[
            project_path.to_string_lossy().into_owned(),
            output_path.to_string_lossy().into_owned(),
        ]).unwrap();

        let dxf = fs::read_to_string(&output_path).unwrap();
        assert!(dxf.contains("$INSUNITS\n70\n4"));
        assert!(dxf.contains("LINE"));
        let manifest_path = output_path.with_extension("dper_measure.json");
        let manifest: serde_json::Value = serde_json::from_slice(&fs::read(manifest_path).unwrap()).unwrap();
        assert_eq!(manifest["capture_id"], "capture-solidcraft");
        assert_eq!(manifest["measurement_count"], 1);
        assert_eq!(manifest["drawing_units"], "millimeter");
    }
}
